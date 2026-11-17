import AppKit
import AVFoundation
import QuartzCore

/// The bot's revenge once he has been teased past bearing: something under the snow wakes. The screen sinks into a
/// dark red murk with ash drifting up through it; dark tentacles creep in from the edges over the wallpaper and wrap
/// it; out of the dark behind the picture a gaunt, rotten Santa looms, his beard running into tentacles, and opens his
/// eyes, red, on whoever kept throwing. Then it all sinks back.
final class Awakening {
    static let duration = 9.0
    /// When his eyes open.
    private static let waking = 4.6

    /// What he is drawn and heard with, made off the main thread: they take a moment.
    struct Pictures {
        let santa: CGImage
        let ember: CGImage
        let ash: CGImage
        let dread: AVAudioPCMBuffer?
    }

    private struct Tentacle {
        let root: CGPoint
        let angle: CGFloat
        let length: CGFloat
        let width: CGFloat
        let phase: CGFloat
        let curl: CGFloat
        let delay: Double
    }

    /// What goes into the wallpaper's own window, behind every other window: the tentacles and him.
    let below = CALayer()
    /// What goes over the whole screen: the murk and the ash.
    let above = CALayer()
    private let santa = CALayer()
    private let eyes = [CALayer(), CALayer()]
    private var tentacles: [(spec: Tentacle, body: CAShapeLayer, vein: CAShapeLayer, suckers: CAShapeLayer)] = []
    private let murk = CAGradientLayer()
    private let flare = CALayer()
    private let ash = CAEmitterLayer()
    private let size: CGSize
    private let unit: CGFloat
    private let dread: AVAudioPCMBuffer?
    private var engine: AVAudioEngine?
    private var window: NSWindow?
    private var timer: Timer?
    private var started = 0.0
    private var finished: () -> Void = {}
    /// Where the pointer is over the screen, in points from its bottom left, for his eyes to follow.
    var pointer: CGPoint

    /// Points to one of his picture's units, a face a little over half one across, for a screen `size` points across.
    static func unit(for size: CGSize) -> CGFloat { size.height * 0.4 }

    /// What he is made of for a screen `size` points across.
    static func pictures(for size: CGSize) -> Pictures? {
        let unit = unit(for: size)
        guard let santa = AwakeningArt.image(AwakeningArt.santaRect, unit: unit * 1.5, AwakeningArt.drawSanta),
              let ember = AwakeningArt.image(AwakeningArt.emberRect, unit: unit * 1.5, AwakeningArt.drawEmber),
              let ash = AwakeningArt.image(CGRect(x: -1, y: -1, width: 2, height: 2), unit: 12, AwakeningArt.drawAsh) else { return nil }
        return Pictures(santa: santa, ember: ember, ash: ash, dread: AwakeningArt.dread(seconds: duration))
    }

    /// Makes what he is made of off the main thread, and hands it over on it.
    static func draw(for size: CGSize, then: @escaping (Pictures?) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let pictures = pictures(for: size)
            DispatchQueue.main.async { then(pictures) }
        }
    }

    /// For a screen `size` points across.
    init(size: CGSize, pictures: Pictures) {
        self.size = size
        unit = Self.unit(for: size)
        dread = pictures.dread
        pointer = CGPoint(x: size.width / 2, y: size.height / 2)
        let still: [String: CAAction] = ["position": NSNull(), "bounds": NSNull(), "transform": NSNull(), "opacity": NSNull(), "contents": NSNull(),
                                         "path": NSNull(), "birthRate": NSNull()]
        for layer in [below, above] {
            layer.frame = CGRect(origin: .zero, size: size)
            layer.actions = still
        }
        let u = unit
        santa.contents = pictures.santa
        santa.contentsGravity = .resize
        let rect = AwakeningArt.santaRect
        santa.bounds = CGRect(x: 0, y: 0, width: rect.width * u, height: rect.height * u)
        santa.anchorPoint = CGPoint(x: -rect.minX / rect.width, y: -rect.minY / rect.height)
        santa.position = CGPoint(x: size.width / 2, y: size.height * 0.6)
        santa.actions = still
        below.addSublayer(santa)
        for eye in eyes {
            eye.contents = pictures.ember
            eye.contentsGravity = .resize
            let ember = AwakeningArt.emberRect
            eye.bounds = CGRect(x: 0, y: 0, width: ember.width * u, height: ember.height * u)
            eye.actions = still
            santa.addSublayer(eye)
        }
        for spec in Self.tentacles(for: size) {
            let body = CAShapeLayer(), vein = CAShapeLayer(), suckers = CAShapeLayer()
            body.fillColor = NSColor(srgbRed: 0.08, green: 0.01, blue: 0.02, alpha: 0.98).cgColor
            body.strokeColor = NSColor(srgbRed: 0.3, green: 0.02, blue: 0.05, alpha: 0.9).cgColor
            body.lineWidth = 1.5
            vein.fillColor = nil
            vein.strokeColor = NSColor(srgbRed: 0.55, green: 0.05, blue: 0.08, alpha: 0.6).cgColor
            vein.lineWidth = spec.width * 0.1
            vein.lineCap = .round
            vein.lineJoin = .round
            suckers.fillColor = NSColor(srgbRed: 0.42, green: 0.05, blue: 0.09, alpha: 0.85).cgColor
            suckers.strokeColor = NSColor(srgbRed: 0.03, green: 0, blue: 0.01, alpha: 0.9).cgColor
            suckers.lineWidth = 1
            for layer in [body, vein, suckers] {
                layer.actions = still
                below.addSublayer(layer)
            }
            tentacles.append((spec, body, vein, suckers))
        }
        murk.type = .radial
        murk.colors = [NSColor(srgbRed: 0.28, green: 0, blue: 0.03, alpha: 0.38).cgColor, NSColor(srgbRed: 0.04, green: 0, blue: 0.01, alpha: 0.82).cgColor]
        murk.startPoint = CGPoint(x: 0.5, y: 0.55)
        murk.endPoint = CGPoint(x: 1.15, y: 1.15)
        murk.frame = above.bounds
        murk.actions = still
        above.addSublayer(murk)
        flare.backgroundColor = NSColor(srgbRed: 0.85, green: 0.05, blue: 0.08, alpha: 1).cgColor
        flare.frame = above.bounds
        flare.actions = still
        above.addSublayer(flare)
        let cell = CAEmitterCell()
        cell.contents = pictures.ash
        cell.birthRate = 22
        cell.lifetime = 8
        cell.velocity = 14
        cell.velocityRange = 10
        cell.yAcceleration = 4
        cell.emissionRange = .pi * 2
        cell.scale = 0.5
        cell.scaleRange = 0.35
        cell.alphaSpeed = -0.08
        cell.spinRange = 1
        cell.color = NSColor(white: 0.85, alpha: 0.7).cgColor
        ash.emitterCells = [cell]
        ash.emitterShape = .rectangle
        ash.emitterMode = .surface
        ash.emitterPosition = CGPoint(x: size.width / 2, y: size.height / 2)
        ash.emitterSize = size
        ash.frame = above.bounds
        ash.birthRate = 0
        ash.actions = still
        above.addSublayer(ash)
        layout(at: 0)
    }

    /// Wakes him: the tentacles and he in `picture`, the wallpaper's window, behind every other window; the murk over
    /// the screen at `frame`, above every window and the Dock. Lets go of it all once he sleeps again.
    func show(in picture: NSWindow, over frame: NSRect, sharing: NSWindow.SharingType, then finished: @escaping () -> Void) {
        picture.contentView?.layer?.addSublayer(below)
        let window = AwakeningWindow(contentRect: frame, styleMask: .borderless, backing: .buffered, defer: false)
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
        view.layer?.addSublayer(above)
        window.setFrame(frame, display: false)
        window.orderFrontRegardless()
        self.window = window
        self.finished = finished
        if let dread {
            let engine = AVAudioEngine(), player = AVAudioPlayerNode()
            engine.attach(player)
            engine.connect(player, to: engine.mainMixerNode, format: dread.format)
            if (try? engine.start()) != nil {
                player.scheduleBuffer(dread, at: nil)
                player.play()
                self.engine = engine
            }
        }
        ash.birthRate = 1
        started = CACurrentMediaTime()
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func tick() {
        let t = CACurrentMediaTime() - started
        if let window {
            let mouse = NSEvent.mouseLocation
            pointer = CGPoint(x: mouse.x - window.frame.minX, y: mouse.y - window.frame.minY)
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layout(at: t)
        if t >= 7 { ash.birthRate = 0 }
        CATransaction.commit()
        guard t >= Self.duration else { return }
        timer?.invalidate()
        timer = nil
        window?.orderOut(nil)
        window = nil
        below.removeFromSuperlayer()
        above.removeFromSuperlayer()
        if let engine {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { engine.stop() }
        }
        engine = nil
        finished()
    }

    /// Where everything is `t` seconds in.
    func layout(at t: Double) {
        func phase(_ from: Double, _ to: Double) -> CGFloat { CGFloat(min(max((t - from) / (to - from), 0), 1)) }
        func ease(_ x: CGFloat) -> CGFloat { x * x * (3 - 2 * x) }
        let fading = ease(phase(7, Self.duration))
        let awake = t >= Self.waking
        murk.opacity = Float(ease(phase(0, 1.6)) * (1 - fading))
        flare.opacity = awake ? Float(0.32 * (1 - phase(Self.waking, Self.waking + 0.5))) : 0
        // The tentacles creep in, writhing harder once he is awake, and draw back as he sleeps again.
        let stir: CGFloat = awake ? 1.6 : 1
        for (spec, body, vein, suckers) in tentacles {
            let length = spec.length * ease(phase(spec.delay, spec.delay + 3)) * (1 - ease(phase(7, 8.6)))
            let shape = Self.shape(of: spec, length: length, t: t, stir: stir)
            body.path = shape?.body
            vein.path = shape?.vein
            suckers.path = shape?.suckers
        }
        // He comes out of the dark, looming nearer, breathing; his eyes open, and follow the pointer.
        let shown = ease(phase(1.5, 4)) * (1 - fading)
        santa.opacity = Float(shown * 0.95)
        let looming = 0.94 + 0.06 * ease(phase(1.5, 5)) + 0.008 * CGFloat(sin(t * 1.3))
        let tilt = awake ? -0.04 * ease(phase(Self.waking + 0.4, Self.waking + 1.6)) : 0
        santa.transform = CATransform3DRotate(CATransform3DMakeScale(looming, looming, 1), tilt, 0, 0, 1)
        let open = max(0.04, ease(phase(Self.waking, Self.waking + 0.45)) * (1 - ease(phase(7.2, 8))))
        let u = unit, rect = AwakeningArt.santaRect
        for (k, eye) in eyes.enumerated() {
            let socket = AwakeningArt.socket(k == 0 ? -1 : 1)
            let seen = CGPoint(x: santa.position.x + socket.x * u * looming, y: santa.position.y + socket.y * u * looming)
            let toward = CGPoint(x: pointer.x - seen.x, y: pointer.y - seen.y)
            let length = max(hypot(toward.x, toward.y), 1)
            let look = CGPoint(x: toward.x / length * 0.025, y: toward.y / length * 0.012)
            eye.position = CGPoint(x: (socket.x + look.x - rect.minX) * u, y: (socket.y + look.y - rect.minY) * u)
            let glow = 1 + 0.08 * CGFloat(sin(t * 5))
            eye.transform = CATransform3DMakeScale(glow, open * glow, 1)
        }
    }

    private static func tentacles(for size: CGSize) -> [Tentacle] {
        var random = AwakeningArt.Random(state: 13)
        var list: [Tentacle] = []
        let middle = CGPoint(x: size.width / 2, y: size.height * 0.55)
        func add(_ root: CGPoint) {
            let toward = atan2(middle.y - root.y, middle.x - root.x) + (random.next() - 0.5) * 0.9
            list.append(Tentacle(root: root, angle: toward, length: size.height * (0.45 + random.next() * 0.45),
                                 width: size.height * (0.045 + random.next() * 0.05), phase: random.next() * 6.3,
                                 curl: (random.next() > 0.5 ? 1 : -1) * (0.6 + random.next()), delay: 0.2 + Double(random.next()) * 1.6))
        }
        for k in 0..<10 { add(CGPoint(x: size.width * (0.02 + 0.96 * (CGFloat(k) + 0.2 + random.next() * 0.6) / 10), y: -20)) }
        for k in 0..<4 {
            add(CGPoint(x: -20, y: size.height * (0.08 + 0.22 * CGFloat(k) + random.next() * 0.08)))
            add(CGPoint(x: size.width + 20, y: size.height * (0.08 + 0.22 * CGFloat(k) + random.next() * 0.08)))
        }
        for x in [0.03, 0.2, 0.8, 0.97] as [CGFloat] { add(CGPoint(x: size.width * x, y: size.height + 20)) }
        return list
    }

    /// A tentacle `length` long: its body, tapering to its tip, writhing as a wave runs along it and curling at its
    /// end; the vein down it, and the suckers along its inner side.
    private static func shape(of spec: Tentacle, length: CGFloat, t: Double, stir: CGFloat) -> (body: CGPath, vein: CGPath, suckers: CGPath)? {
        guard length > 2 else { return nil }
        let count = 36
        let step = length / CGFloat(count)
        var angle = spec.angle, point = spec.root
        var left: [CGPoint] = [], right: [CGPoint] = [], spine: [CGPoint] = []
        for i in 0...count {
            let f = CGFloat(i) / CGFloat(count)
            let half = spec.width * pow(1 - f, 0.75) * 0.5 + 0.8
            let normal = CGPoint(x: -sin(angle), y: cos(angle))
            left.append(CGPoint(x: point.x + normal.x * half, y: point.y + normal.y * half))
            right.append(CGPoint(x: point.x - normal.x * half, y: point.y - normal.y * half))
            spine.append(point)
            angle += 0.09 * stir * sin(f * 9 - CGFloat(t) * 2.2 * stir + spec.phase) + spec.curl * f * f * f * 0.25
            point = CGPoint(x: point.x + cos(angle) * step, y: point.y + sin(angle) * step)
        }
        let body = CGMutablePath()
        body.addLines(between: left + right.reversed())
        body.closeSubpath()
        let vein = CGMutablePath()
        vein.addLines(between: Array(spine.prefix(Int(Double(count) * 0.85))))
        let suckers = CGMutablePath()
        for i in stride(from: 2, to: count - 3, by: 2) {
            let f = CGFloat(i) / CGFloat(count)
            let r = spec.width * pow(1 - f, 0.75) * 0.13
            let inner = spec.curl > 0 ? left[i] : right[i]
            let middle = CGPoint(x: spine[i].x + (inner.x - spine[i].x) * 0.62, y: spine[i].y + (inner.y - spine[i].y) * 0.62)
            suckers.addEllipse(in: CGRect(x: middle.x - r, y: middle.y - r, width: r * 2, height: r * 2))
        }
        return (body, vein, suckers)
    }
}

private final class AwakeningWindow: NSWindow {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    // Borderless windows are otherwise pushed below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}

/// His pictures and the sound of him waking. Pictures are in units of a little under twice his face across, from the
/// middle of his face.
enum AwakeningArt {
    static let santaRect = CGRect(x: -0.95, y: -1.35, width: 1.9, height: 2.3)
    static let emberRect = CGRect(x: -0.09, y: -0.045, width: 0.18, height: 0.09)

    /// The middle of his eye socket on a side, -1 his right as seen, 1 his left.
    static func socket(_ side: CGFloat) -> CGPoint { CGPoint(x: side * 0.13, y: 0.0) }

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

    private static func radial(_ context: CGContext, at point: CGPoint, radius: CGFloat, _ colors: [CGColor], _ locations: [CGFloat]? = nil) {
        guard let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors as CFArray, locations: locations) else { return }
        context.drawRadialGradient(gradient, startCenter: point, startRadius: 0, endCenter: point, endRadius: radius, options: [])
    }

    /// Numbers that look picked at random, the same each time.
    struct Random {
        var state: UInt64
        mutating func next() -> CGFloat {
            state = state &* 6364136223846793005 &+ 1442695040888963407
            return CGFloat(state >> 33) / CGFloat(UInt64(1) << 31)
        }
    }

    /// A hair or a root: a strand from `start` down along `angle`, wavering and curling, thinning from `width`, its
    /// colour running from `top` to `bottom` along it.
    private static func strand(_ context: CGContext, from start: CGPoint, angle: CGFloat, length: CGFloat, width: CGFloat, curl: CGFloat,
                               top: (CGFloat, CGFloat, CGFloat, CGFloat), bottom: (CGFloat, CGFloat, CGFloat, CGFloat), turnAt: CGFloat,
                               random: inout Random) {
        let count = 18
        var point = start, heading = angle
        let wave = random.next() * 6.3
        context.setLineCap(.round)
        for i in 0..<count {
            let f = CGFloat(i) / CGFloat(count)
            heading += 0.12 * sin(f * 8 + wave) + curl * f * f * 0.22
            let next = CGPoint(x: point.x + cos(heading) * length / CGFloat(count), y: point.y + sin(heading) * length / CGFloat(count))
            let mix = min(max((f - turnAt) / 0.25, 0), 1)
            context.setStrokeColor(color(top.0 + (bottom.0 - top.0) * mix, top.1 + (bottom.1 - top.1) * mix, top.2 + (bottom.2 - top.2) * mix,
                                         top.3 + (bottom.3 - top.3) * mix))
            context.setLineWidth(width * (1 - 0.75 * f))
            context.move(to: point)
            context.addLine(to: next)
            context.strokePath()
            point = next
        }
    }

    static func drawSanta(_ context: CGContext) {
        var random = Random(state: 23)
        // The dark he comes out of.
        radial(context, at: CGPoint(x: 0, y: -0.15), radius: 1.05, [color(0, 0, 0, 0.92), color(0, 0, 0, 0.7), color(0, 0, 0, 0)], [0, 0.55, 1])
        // His coat at the shoulders, rotten red going to black.
        let coat = CGMutablePath()
        coat.move(to: CGPoint(x: -0.95, y: -1.35))
        coat.addCurve(to: CGPoint(x: -0.2, y: -0.42), control1: CGPoint(x: -0.8, y: -0.75), control2: CGPoint(x: -0.45, y: -0.5))
        coat.addLine(to: CGPoint(x: 0.2, y: -0.42))
        coat.addCurve(to: CGPoint(x: 0.95, y: -1.35), control1: CGPoint(x: 0.45, y: -0.5), control2: CGPoint(x: 0.8, y: -0.75))
        coat.closeSubpath()
        context.saveGState()
        context.addPath(coat)
        context.clip()
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [color(0.3, 0.02, 0.05), color(0.03, 0, 0.01)] as CFArray, locations: nil) {
            context.drawLinearGradient(gradient, start: CGPoint(x: 0, y: -0.45), end: CGPoint(x: 0, y: -1.3), options: [])
        }
        context.restoreGState()
        // His face: long and gaunt, all bone, grey, the cheeks fallen in, veins dark under the skin.
        let face = CGMutablePath()
        face.move(to: CGPoint(x: 0, y: 0.24))
        face.addCurve(to: CGPoint(x: 0.31, y: 0.0), control1: CGPoint(x: 0.2, y: 0.24), control2: CGPoint(x: 0.31, y: 0.14))
        face.addCurve(to: CGPoint(x: 0.17, y: -0.38), control1: CGPoint(x: 0.31, y: -0.12), control2: CGPoint(x: 0.22, y: -0.24))
        face.addCurve(to: CGPoint(x: 0, y: -0.5), control1: CGPoint(x: 0.13, y: -0.46), control2: CGPoint(x: 0.07, y: -0.5))
        face.addCurve(to: CGPoint(x: -0.17, y: -0.38), control1: CGPoint(x: -0.07, y: -0.5), control2: CGPoint(x: -0.13, y: -0.46))
        face.addCurve(to: CGPoint(x: -0.31, y: 0.0), control1: CGPoint(x: -0.22, y: -0.24), control2: CGPoint(x: -0.31, y: -0.12))
        face.addCurve(to: CGPoint(x: 0, y: 0.24), control1: CGPoint(x: -0.31, y: 0.14), control2: CGPoint(x: -0.2, y: 0.24))
        context.saveGState()
        context.addPath(face)
        context.clip()
        radial(context, at: CGPoint(x: 0, y: -0.02), radius: 0.4, [color(0.66, 0.62, 0.61), color(0.42, 0.37, 0.39), color(0.14, 0.11, 0.13)], [0, 0.6, 1])
        for side in [-1.0, 1.0] as [CGFloat] {
            radial(context, at: CGPoint(x: side * 0.2, y: -0.2), radius: 0.15, [color(0.04, 0.01, 0.03, 0.78), color(0.04, 0.01, 0.03, 0)])
            radial(context, at: CGPoint(x: side * 0.28, y: 0.08), radius: 0.11, [color(0.04, 0.01, 0.03, 0.6), color(0.04, 0.01, 0.03, 0)])
            // The cheekbone standing out over the hollow.
            radial(context, at: CGPoint(x: side * 0.2, y: -0.06), radius: 0.07, [color(0.8, 0.76, 0.74, 0.35), color(0.8, 0.76, 0.74, 0)])
        }
        for _ in 0..<700 {
            let p = CGPoint(x: (random.next() - 0.5) * 0.62, y: -0.46 + random.next() * 0.68)
            let r = 0.0015 + random.next() * 0.004
            context.setFillColor(random.next() > 0.5 ? color(0.85, 0.82, 0.8, 0.15) : color(0.1, 0.05, 0.06, 0.22))
            context.fillEllipse(in: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 2))
        }
        context.setLineCap(.round)
        for _ in 0..<26 {
            let around = random.next() * 2 * .pi
            var point = CGPoint(x: cos(around) * 0.3, y: -0.12 + sin(around) * 0.33)
            var heading = around + .pi + (random.next() - 0.5) * 1.2
            var width: CGFloat = 0.009
            for _ in 0..<9 {
                heading += (random.next() - 0.5) * 0.9
                let next = CGPoint(x: point.x + cos(heading) * 0.03, y: point.y + sin(heading) * 0.03)
                context.setStrokeColor(color(0.42, 0.03, 0.08, 0.7))
                context.setLineWidth(width)
                context.move(to: point)
                context.addLine(to: next)
                context.strokePath()
                if random.next() > 0.75 {
                    let fork = heading + (random.next() > 0.5 ? 0.8 : -0.8)
                    context.setLineWidth(width * 0.6)
                    context.move(to: next)
                    context.addLine(to: CGPoint(x: next.x + cos(fork) * 0.035, y: next.y + sin(fork) * 0.035))
                    context.strokePath()
                }
                point = next
                width *= 0.9
            }
        }
        context.restoreGState()
        // Deep sockets, the eyes in them shut.
        for side in [-1.0, 1.0] as [CGFloat] {
            let middle = socket(side)
            radial(context, at: middle, radius: 0.14, [color(0.02, 0, 0.01, 0.7), color(0.02, 0, 0.01, 0)])
            context.saveGState()
            context.translateBy(x: middle.x, y: middle.y)
            context.rotate(by: side * 0.25)
            context.setFillColor(color(0.02, 0.0, 0.01))
            context.fillEllipse(in: CGRect(x: -0.1, y: -0.06, width: 0.2, height: 0.12))
            context.restoreGState()
            // Matted brows, bristling down to the nose.
            for _ in 0..<26 {
                let x = side * (0.04 + random.next() * 0.22)
                let y = 0.06 + (abs(x) - 0.04) * 0.25 + random.next() * 0.03
                context.setStrokeColor(color(0.72, 0.7, 0.66, 0.8))
                context.setLineWidth(0.006 + random.next() * 0.004)
                context.move(to: CGPoint(x: x, y: y))
                context.addLine(to: CGPoint(x: x - side * (0.03 + random.next() * 0.02), y: y - 0.01 - random.next() * 0.025))
                context.strokePath()
            }
        }
        // A long bony nose.
        context.setStrokeColor(color(0.1, 0.05, 0.06, 0.6))
        context.setLineWidth(0.008)
        context.move(to: CGPoint(x: -0.022, y: 0.01))
        context.addQuadCurve(to: CGPoint(x: -0.035, y: -0.13), control: CGPoint(x: -0.03, y: -0.08))
        context.move(to: CGPoint(x: 0.022, y: 0.01))
        context.addQuadCurve(to: CGPoint(x: 0.035, y: -0.13), control: CGPoint(x: 0.03, y: -0.08))
        context.strokePath()
        context.setFillColor(color(0.03, 0.01, 0.02))
        for side in [-1.0, 1.0] as [CGFloat] { context.fillEllipse(in: CGRect(x: side * 0.026 - 0.013, y: -0.165, width: 0.026, height: 0.016)) }
        // A mouth hanging open, dark, a few long teeth in it.
        let mouth = CGRect(x: -0.09, y: -0.32, width: 0.18, height: 0.09)
        context.setFillColor(color(0.03, 0.0, 0.01))
        context.fillEllipse(in: mouth)
        context.saveGState()
        context.addEllipse(in: mouth)
        context.clip()
        context.setFillColor(color(0.62, 0.58, 0.5))
        for x in [-0.055, -0.02, 0.025, 0.06] as [CGFloat] {
            context.addLines(between: [CGPoint(x: x - 0.012, y: -0.22), CGPoint(x: x + 0.012, y: -0.22), CGPoint(x: x + 0.002, y: -0.27 - random.next() * 0.02)])
            context.closePath()
        }
        context.fillPath()
        context.restoreGState()
        // His beard: long and matted, dirty white, running down into dark red tentacles.
        for _ in 0..<110 {
            let along = random.next() * 2 - 1
            let start = CGPoint(x: along * 0.27, y: -0.42 + abs(along) * 0.14 + random.next() * 0.04)
            strand(context, from: start, angle: -.pi / 2 + along * 0.35 + (random.next() - 0.5) * 0.3, length: 0.55 + random.next() * 0.45,
                   width: 0.012 + random.next() * 0.008, curl: (random.next() - 0.5) * 6, top: (0.78, 0.75, 0.69, 0.92), bottom: (0.16, 0.02, 0.03, 0.97),
                   turnAt: 0.35 + random.next() * 0.2, random: &random)
        }
        // The beard's ends gone to thick dark tentacles, reaching down and out.
        for k in 0..<10 {
            let along = CGFloat(k) / 9 * 2 - 1
            let start = CGPoint(x: along * 0.22, y: -0.75 - random.next() * 0.1)
            strand(context, from: start, angle: -.pi / 2 + along * 0.7, length: 0.45 + random.next() * 0.3, width: 0.045 + random.next() * 0.02,
                   curl: (random.next() > 0.5 ? 1 : -1) * (3 + random.next() * 3), top: (0.2, 0.02, 0.04, 0.0), bottom: (0.1, 0.01, 0.02, 0.97),
                   turnAt: 0.0, random: &random)
        }
        // His moustache, drooping over the mouth.
        for side in [-1.0, 1.0] as [CGFloat] {
            for _ in 0..<34 {
                let start = CGPoint(x: side * (0.01 + random.next() * 0.04), y: -0.17 - random.next() * 0.02)
                strand(context, from: start, angle: -.pi / 2 + side * (0.7 + random.next() * 0.5), length: 0.16 + random.next() * 0.1, width: 0.008,
                       curl: side * 2, top: (0.82, 0.79, 0.73, 0.95), bottom: (0.6, 0.57, 0.52, 0.9), turnAt: 0.5, random: &random)
            }
        }
        // His cap, rotten red and torn, tall, its tip flopped over and hanging down at his side.
        let hood = CGMutablePath()
        hood.move(to: CGPoint(x: -0.38, y: 0.16))
        hood.addCurve(to: CGPoint(x: 0.08, y: 0.9), control1: CGPoint(x: -0.33, y: 0.5), control2: CGPoint(x: -0.12, y: 0.9))
        hood.addCurve(to: CGPoint(x: 0.66, y: 0.1), control1: CGPoint(x: 0.4, y: 0.9), control2: CGPoint(x: 0.68, y: 0.5))
        hood.addCurve(to: CGPoint(x: 0.36, y: 0.18), control1: CGPoint(x: 0.56, y: 0.32), control2: CGPoint(x: 0.42, y: 0.5))
        hood.closeSubpath()
        context.saveGState()
        context.addPath(hood)
        context.clip()
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: [color(0.42, 0.03, 0.06), color(0.12, 0, 0.02)] as CFArray, locations: nil) {
            context.drawLinearGradient(gradient, start: CGPoint(x: -0.2, y: 0.6), end: CGPoint(x: 0.5, y: 0.0), options: [])
        }
        for _ in 0..<9 {
            let p = CGPoint(x: -0.3 + random.next() * 0.85, y: 0.2 + random.next() * 0.55)
            let r = 0.012 + random.next() * 0.025
            context.setFillColor(color(0.02, 0, 0.01, 0.85))
            context.fillEllipse(in: CGRect(x: p.x - r, y: p.y - r * 0.7, width: r * 2, height: r * 1.4))
        }
        context.restoreGState()
        // The fur at its brim and its tip, gone grey and filthy.
        for rect in [CGRect(x: -0.4, y: 0.1, width: 0.8, height: 0.13), CGRect(x: 0.6, y: 0.0, width: 0.13, height: 0.13)] {
            SnowmanArt.fur(context, in: rect, seed: 31)
            context.setFillColor(color(0.24, 0.19, 0.15, 0.55))
            context.fillEllipse(in: rect)
        }
    }

    static func drawEmber(_ context: CGContext) {
        context.scaleBy(x: 1, y: 0.5)
        radial(context, at: .zero, radius: 0.09, [color(1, 0.95, 0.6), color(1, 0.2, 0.04), color(0.7, 0, 0, 0)], [0, 0.35, 1])
    }

    static func drawAsh(_ context: CGContext) {
        radial(context, at: .zero, radius: 1, [color(1, 1, 1, 0.9), color(1, 1, 1, 0)])
    }

    /// The sound of him waking, `seconds` long: a low drone and a rumble swelling up, a heart beating, and as his eyes
    /// open a deep growl; all dying away.
    static func dread(seconds: Double) -> AVAudioPCMBuffer? {
        let rate = 44_100.0
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
        var throat = [Resonance(320, q: 2, rate: rate), Resonance(850, q: 3, rate: rate)]
        var noise: UInt64 = 11
        var rumble = 0.0, growlPhase = 0.0
        let beats = [2.4, 3.25, 4.0]
        let waking = 4.6
        for i in 0..<Int(buffer.frameLength) {
            let t = Double(i) / rate
            noise = noise &* 6364136223846793005 &+ 1442695040888963407
            let white = Double(noise >> 11) / Double(UInt64(1) << 52) - 1
            let swell = min(t / 2, 1) * (t < seconds - 2 ? 1 : max(0, (seconds - t) / 2))
            var drone = 0.32 * sin(2 * .pi * 36.7 * t + 0.3 * sin(2 * .pi * 0.13 * t)) + 0.24 * sin(2 * .pi * 55 * t)
                + 0.14 * sin(2 * .pi * 73.4 * t + 0.5 * sin(2 * .pi * 0.07 * t)) + 0.06 * sin(2 * .pi * 110.3 * t)
            drone *= 0.75 + 0.25 * sin(2 * .pi * 0.21 * t)
            rumble += (white - rumble) * 0.02
            var heart = 0.0
            for beat in beats {
                for (offset, loud) in [(0.0, 0.9), (0.2, 0.6)] {
                    let tau = t - beat - offset
                    if tau >= 0 && tau < 0.25 { heart += loud * sin(2 * .pi * (58 - 25 * tau) * tau) * exp(-tau * 26) }
                }
            }
            var growl = 0.0
            let tg = t - waking
            if tg >= 0 && tg < 2.2 {
                let envelope = min(tg / 0.25, 1) * exp(-max(tg - 0.5, 0) * 1.6)
                growlPhase += (58 + 6 * sin(2 * .pi * 9 * t) + 5 * white) / rate
                growlPhase -= floor(growlPhase)
                let voice = (2 * growlPhase - 1) + 0.6 * white
                var shaped = 0.2 * voice
                shaped += 1.4 * throat[0].run(voice) + 0.9 * throat[1].run(voice)
                growl = 0.75 * tanh(3 * shaped) * envelope
            }
            let sample = Float(tanh(1.2 * (swell * (0.55 * drone + 1.8 * rumble) + heart + growl)))
            channels[0][i] = sample
            channels[1][i] = sample
        }
        return buffer
    }
}
