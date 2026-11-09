import AppKit
import QuartzCore

/// Shown on first launch. Getting started crumbles the window into snow that falls onto the desktop.
final class WelcomeController: NSObject {
    /// Receives the pieces of the window as it crumbles, in global screen coordinates.
    var scatter: ([ScatterPiece]) -> Void = { _ in }
    var finished: () -> Void = {}
    /// Called instead of `scatter` when the window can't be captured and just closes.
    var skipped: () -> Void = {}

    private var window: NSWindow?
    private var crumble: Crumble?

    func show() {
        let size = CGSize(width: 440, height: 430)
        let window = WelcomeWindow(
            contentRect: CGRect(origin: .zero, size: size),
            styleMask: .borderless,
            backing: .buffered,
            defer: false
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.level = .floating
        window.isMovableByWindowBackground = true
        window.isReleasedWhenClosed = false
        window.onCancel = { [weak self] in self?.getStarted() }

        let content = WelcomeView(frame: CGRect(origin: .zero, size: size))
        content.button.target = self
        content.button.action = #selector(getStarted)
        window.contentView = content
        window.center()
        self.window = window

        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    @objc private func getStarted() {
        guard crumble == nil, let window, let view = window.contentView as? WelcomeView else { return }
        view.button.isEnabled = false
        guard let crumble = Crumble(view: view) else {
            window.orderOut(nil)
            skipped()
            finished()
            return
        }
        self.crumble = crumble
        let windowOrigin = window.frame.origin
        crumble.start { [weak self] cells in
            guard let self else { return }
            let center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
            self.scatter(cells.map { cell in
                let x = Float(windowOrigin.x + cell.point.x)
                let y = Float(windowOrigin.y + cell.point.y)
                let outward = Float((cell.point.x - center.x) / view.bounds.width)
                return ScatterPiece(
                    position: SIMD2(x, y),
                    velocity: SIMD2(outward * 140 + .random(in: -70...70), .random(in: 30...150)),
                    color: cell.color
                )
            })
            window.invalidateShadow()
        } completion: { [weak self] in
            window.orderOut(nil)
            self?.crumble = nil
            self?.window = nil
            self?.finished()
        }
    }
}

private final class WelcomeWindow: NSWindow {
    var onCancel: () -> Void = {}

    override var canBecomeKey: Bool { true }

    override func cancelOperation(_ sender: Any?) {
        onCancel()
    }
}

/// Erases a view from the top down along a ragged, soft-edged front and hands each bit it passes,
/// with its colour, to `pieces`, so the window seems to fall apart into the snow.
private final class Crumble {
    struct Cell {
        let point: CGPoint
        let color: SIMD3<Float>
        let delay: Double
    }

    private let view: NSView
    /// Spacing of the pieces that fly off.
    private let cell: CGFloat = 6
    /// Spacing of the mask: fine enough that its edge reads as a line, not as squares.
    private let grain: CGFloat = 2
    /// Seconds over which a spot of the mask fades out once the front reaches it.
    private let feather = 0.05
    private let columns: Int
    private let rows: Int
    /// When the front reaches each mask pixel, top row first.
    private let front: [Double]
    private var cells: [Cell]
    private var next = 0
    private var pixels: [UInt8]
    private let mask = CALayer()
    private var timer: Timer?
    private var started: CFTimeInterval = 0
    private let duration: Double

    init?(view: NSView) {
        guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: rep)
        self.view = view
        let bounds = view.bounds
        columns = Int((bounds.width / grain).rounded(.up))
        rows = Int((bounds.height / grain).rounded(.up))
        pixels = Array(repeating: 255, count: columns * rows * 4)

        var front = [Double](repeating: 0, count: columns * rows)
        for row in 0..<rows {
            for column in 0..<columns {
                let point = CGPoint(x: (CGFloat(column) + 0.5) * grain, y: bounds.height - (CGFloat(row) + 0.5) * grain)
                front[row * columns + column] = Crumble.arrival(at: point, in: bounds)
            }
        }
        self.front = front

        let scale = CGFloat(rep.pixelsWide) / bounds.width
        var cells: [Cell] = []
        for y in stride(from: cell / 2, to: bounds.height, by: cell) {
            for x in stride(from: cell / 2, to: bounds.width, by: cell) {
                let point = CGPoint(x: x, y: y)
                // Bitmap rows run top-down; the view's run bottom-up.
                let px = min(Int(x * scale), rep.pixelsWide - 1)
                let py = min(Int((bounds.height - y) * scale), rep.pixelsHigh - 1)
                guard let sample = rep.colorAt(x: px, y: py)?.usingColorSpace(.sRGB), sample.alphaComponent > 0.1 else { continue }
                cells.append(Cell(
                    point: point,
                    color: SIMD3(Float(sample.redComponent), Float(sample.greenComponent), Float(sample.blueComponent)),
                    delay: Crumble.arrival(at: point, in: bounds)
                ))
            }
        }
        cells.sort { $0.delay < $1.delay }
        self.cells = cells
        duration = (front.max() ?? 0) + feather
    }

    func start(pieces: @escaping ([Cell]) -> Void, completion: @escaping () -> Void) {
        view.wantsLayer = true
        mask.frame = view.bounds
        mask.contentsGravity = .resize
        view.layer?.mask = mask
        updateMask(at: 0)
        started = CACurrentMediaTime()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60, repeats: true) { [weak self] timer in
            guard let self else {
                timer.invalidate()
                return
            }
            let elapsed = CACurrentMediaTime() - self.started
            var batch: [Cell] = []
            while self.next < self.cells.count, self.cells[self.next].delay <= elapsed {
                batch.append(self.cells[self.next])
                self.next += 1
            }
            self.updateMask(at: elapsed)
            if !batch.isEmpty { pieces(batch) }
            if elapsed >= self.duration {
                timer.invalidate()
                completion()
            }
        }
    }

    /// The front sweeps down in about half a second, held back here and there by smooth noise
    /// and roughened by a fine grain so its edge looks like crumbling snow.
    private static func arrival(at point: CGPoint, in bounds: CGRect) -> Double {
        let fromTop = 1 - Double(point.y / bounds.height)
        let x = Double(point.x), y = Double(point.y)
        let broad = valueNoise(x / 46, y / 46) * 0.65 + valueNoise(x / 15 + 7.3, y / 15 + 2.1) * 0.35
        return fromTop * 0.5 + broad * 0.2 + hash(Int(x * 1.7), Int(y * 1.7)) * 0.035
    }

    private func updateMask(at time: Double) {
        for i in 0..<front.count {
            let a = UInt8(max(0, min(1, (front[i] - time) / feather)) * 255)
            pixels[i * 4] = a
            pixels[i * 4 + 1] = a
            pixels[i * 4 + 2] = a
            pixels[i * 4 + 3] = a
        }
        guard let provider = CGDataProvider(data: Data(pixels) as CFData),
              let image = CGImage(
                width: columns,
                height: rows,
                bitsPerComponent: 8,
                bitsPerPixel: 32,
                bytesPerRow: columns * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider,
                decode: nil,
                shouldInterpolate: true,
                intent: .defaultIntent
              )
        else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        mask.contents = image
        CATransaction.commit()
    }

    private static func valueNoise(_ x: Double, _ y: Double) -> Double {
        let x0 = Int(floor(x)), y0 = Int(floor(y))
        let fx = x - floor(x), fy = y - floor(y)
        let u = fx * fx * (3 - 2 * fx), v = fy * fy * (3 - 2 * fy)
        let top = hash(x0, y0) + (hash(x0 + 1, y0) - hash(x0, y0)) * u
        let bottom = hash(x0, y0 + 1) + (hash(x0 + 1, y0 + 1) - hash(x0, y0 + 1)) * u
        return top + (bottom - top) * v
    }

    private static func hash(_ x: Int, _ y: Int) -> Double {
        var n = UInt32(truncatingIfNeeded: x &* 374_761_393 &+ y &* 668_265_263)
        n = (n ^ (n >> 13)) &* 1_274_126_177
        n ^= n >> 16
        return Double(n & 0xFFFF) / 65535
    }
}

/// Drawn rather than built from layer properties, so `cacheDisplay` captures exactly what is on screen.
private final class WelcomeView: NSView {
    let button = GetStartedButton()
    private let background = NSColor(srgbRed: 0.105, green: 0.088, blue: 0.072, alpha: 1)
    private let ink = NSColor(srgbRed: 0.93, green: 0.9, blue: 0.85, alpha: 1)
    private let muted = NSColor(srgbRed: 0.78, green: 0.75, blue: 0.7, alpha: 1)

    override init(frame: NSRect) {
        super.init(frame: frame)

        // The bundled artwork, not applicationIconImage, which follows the system's tinted or clear icon style.
        let icon = NSImageView(image: Bundle.main.image(forResource: "AppIcon") ?? NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([icon.widthAnchor.constraint(equalToConstant: 112), icon.heightAnchor.constraint(equalToConstant: 112)])

        let title = NSTextField(labelWithAttributedString: NSAttributedString(
            string: "IT'S SNOWING\nON YOUR DESKTOP.",
            attributes: [
                .font: NSFont.systemFont(ofSize: 34, weight: .light, width: .condensed),
                .foregroundColor: ink,
                .kern: 0.6,
                .paragraphStyle: Self.paragraph(lineHeight: 0.95),
            ]
        ))

        let body = NSTextField(wrappingLabelWithString: tr("welcome.mac"))
        body.attributedStringValue = NSAttributedString(string: body.stringValue, attributes: [
            .font: NSFont.systemFont(ofSize: 13.5),
            .foregroundColor: muted,
            .paragraphStyle: Self.paragraph(lineHeight: 1.3),
        ])
        body.preferredMaxLayoutWidth = 330
        // Selectable labels swap in a field editor on click, which re-lays the text out without its paragraph style.
        title.isSelectable = false
        body.isSelectable = false

        let stack = NSStackView(views: [icon, title, body, button])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 18
        stack.setCustomSpacing(10, after: icon)
        stack.setCustomSpacing(28, after: body)
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor),
            stack.widthAnchor.constraint(lessThanOrEqualTo: widthAnchor, constant: -64),
        ])
    }

    required init?(coder: NSCoder) { nil }

    override var mouseDownCanMoveWindow: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        let shape = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 18, yRadius: 18)
        background.setFill()
        shape.fill()
        NSColor(white: 1, alpha: 0.08).setStroke()
        shape.lineWidth = 1
        shape.stroke()
    }

    private static func paragraph(lineHeight: CGFloat) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        style.lineHeightMultiple = lineHeight
        return style
    }
}

private final class GetStartedButton: NSButton {
    private let fill = NSColor(srgbRed: 0.93, green: 0.9, blue: 0.85, alpha: 1)
    private let label = NSColor(srgbRed: 0.09, green: 0.075, blue: 0.06, alpha: 1)

    init() {
        super.init(frame: .zero)
        title = tr("welcome.start")
        isBordered = false
        keyEquivalent = "\r"
        focusRingType = .none
        translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([widthAnchor.constraint(equalToConstant: 190), heightAnchor.constraint(equalToConstant: 44)])
        setAccessibilityLabel(tr("welcome.start"))
    }

    required init?(coder: NSCoder) { nil }

    override func draw(_ dirtyRect: NSRect) {
        let shape = NSBezierPath(roundedRect: bounds, xRadius: 2, yRadius: 2)
        (isHighlighted ? fill.blended(withFraction: 0.12, of: .black) ?? fill : fill).setFill()
        shape.fill()
        let text = NSAttributedString(string: tr("welcome.start").localizedUppercase, attributes: [
            .font: NSFont.systemFont(ofSize: 12.5, weight: .semibold),
            .foregroundColor: label,
            .kern: 1.8,
        ])
        let size = text.size()
        text.draw(at: CGPoint(x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2))
    }
}
