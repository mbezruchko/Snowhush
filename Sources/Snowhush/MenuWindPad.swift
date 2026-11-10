import AppKit

/// Sets the wind by dragging out from the middle of a square: the farther out, the stronger, at most to the ring the
/// dots fill. Back in the middle, or double-clicked, it is calm. Like `MenuSlider`, it follows the pointer itself.
final class MenuWindPad: NSView {
    var changed: ((Wind) -> Void)?
    /// Dragged while the wind wandered: it blows as set from now on.
    var stoppedWandering: (() -> Void)?

    var wind = Wind.calm {
        didSet { needsDisplay = true }
    }

    /// Shows the wind wandering on its own, as it blows over the snow, until dragged.
    var wanders = false {
        didSet {
            if wanders { followWandering() }
            updateTimer()
        }
    }

    /// Whether the menu is open: only then does the wandering wind need following.
    var showing = false {
        didSet {
            if showing && wanders { followWandering() }
            updateTimer()
        }
    }

    private var timer: Timer?

    private var dragging = false {
        didSet { needsDisplay = true }
    }

    private static let side: CGFloat = 156
    private static let top: CGFloat = 2
    private static let bottom: CGFloat = 8
    /// The handle's ring, which stays inside the square at full strength.
    private static let handle: CGFloat = 11
    private static var reach: CGFloat { side / 2 - handle - 5 }

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: Self.top + Self.side + Self.bottom))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.slider)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    private var square: NSRect {
        NSRect(x: ((bounds.width - Self.side) / 2).rounded(), y: Self.top, width: Self.side, height: Self.side)
    }

    private var middle: NSPoint { NSPoint(x: square.midX, y: square.midY) }

    private func updateTimer() {
        let live = wanders && showing
        guard live != (timer != nil) else { return }
        timer?.invalidate()
        timer = nil
        guard live else { return }
        let timer = Timer(timeInterval: 1.0 / 20, repeats: true) { [weak self] _ in self?.followWandering() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func followWandering() {
        wind = Wind.wandering(at: ProcessInfo.processInfo.systemUptime)
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 {
            set(.calm)
            return
        }
        dragging = true
        follow(event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard dragging else { return }
        follow(event)
    }

    override func mouseUp(with event: NSEvent) { dragging = false }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        dragging = false
    }

    /// Moves the handle to the pointer: strength in steps of 5%, calm near the middle, and the direction caught on
    /// the eight compass points within a few degrees of one.
    private func follow(_ event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let x = (point.x - middle.x) / Self.reach
        let y = (middle.y - point.y) / Self.reach
        let strength = (min(hypot(x, y), 1) * 20).rounded() / 20
        guard strength >= 0.1 else {
            set(Wind(strength: 0, direction: wind.direction))
            return
        }
        var direction = atan2(x, y) * 180 / .pi
        let compass = (direction / 45).rounded() * 45
        direction = abs(direction - compass) < 5 ? compass : direction.rounded()
        set(Wind(strength: strength, direction: direction))
    }

    private func set(_ next: Wind) {
        if wanders {
            wanders = false
            stoppedWandering?()
        }
        guard next != wind else { return }
        wind = next
        changed?(next)
    }

    override func accessibilityLabel() -> String? { tr("wind") }

    override func accessibilityValue() -> Any? { wind.summary }

    override func accessibilityPerformIncrement() -> Bool {
        set(Wind(strength: wind.strength + 0.05, direction: wind.direction))
        return true
    }

    override func accessibilityPerformDecrement() -> Bool {
        set(Wind(strength: wind.strength - 0.05, direction: wind.direction))
        return true
    }

    override func draw(_ dirtyRect: NSRect) {
        let square = square
        let middle = middle
        let reach = Self.reach
        let ink = NSColor.labelColor

        let well = NSBezierPath(roundedRect: square.insetBy(dx: 0.5, dy: 0.5), xRadius: 10, yRadius: 10)
        ink.withAlphaComponent(0.06).setFill()
        well.fill()
        (dragging ? ink : NSColor.separatorColor).setStroke()
        well.lineWidth = 1
        well.stroke()

        ink.withAlphaComponent(0.3).setFill()
        let spacing = reach / 3
        for i in -3...3 {
            for j in -3...3 where i * i + j * j <= 10 {
                let dot = NSPoint(x: middle.x + CGFloat(i) * spacing, y: middle.y + CGFloat(j) * spacing)
                NSBezierPath(ovalIn: NSRect(x: dot.x - 1.5, y: dot.y - 1.5, width: 3, height: 3)).fill()
            }
        }

        let vector = wind.vector
        let tip = NSPoint(x: middle.x + CGFloat(vector.x) * reach, y: middle.y - CGFloat(vector.y) * reach)
        ink.withAlphaComponent(dragging ? 0.28 : 0.16).setFill()
        let ring = Self.handle
        NSBezierPath(ovalIn: NSRect(x: tip.x - ring, y: tip.y - ring, width: ring * 2, height: ring * 2)).fill()

        ink.set()
        let length = hypot(tip.x - middle.x, tip.y - middle.y)
        guard length > 0.5 else {
            NSBezierPath(ovalIn: NSRect(x: middle.x - 3.5, y: middle.y - 3.5, width: 7, height: 7)).fill()
            return
        }
        let along = NSPoint(x: (tip.x - middle.x) / length, y: (tip.y - middle.y) / length)
        let head = min(9, length * 0.7)
        let wing = head * 0.62
        let base = NSPoint(x: tip.x - along.x * head, y: tip.y - along.y * head)

        let shaft = NSBezierPath()
        shaft.move(to: middle)
        shaft.line(to: NSPoint(x: base.x + along.x, y: base.y + along.y))
        shaft.lineWidth = 3
        shaft.lineCapStyle = .round
        shaft.stroke()

        let arrow = NSBezierPath()
        arrow.move(to: tip)
        arrow.line(to: NSPoint(x: base.x - along.y * wing, y: base.y + along.x * wing))
        arrow.line(to: NSPoint(x: base.x + along.y * wing, y: base.y - along.x * wing))
        arrow.close()
        arrow.lineJoinStyle = .round
        arrow.lineWidth = 1.5
        arrow.fill()
        arrow.stroke()
    }
}
