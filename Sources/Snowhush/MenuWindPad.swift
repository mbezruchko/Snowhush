import AppKit

/// Sets the wind by dragging out from the middle of a field as wide as the menu: the farther out, the stronger, at
/// most out to the dots' outer ring, which lies farther across than up and down. Back in the middle, or
/// double-clicked, it is calm. Past the field's right edge, set apart by a tick as on the Intensity slider, a last
/// stop lets the wind wander on its own; the field then shows it as it blows. Like `MenuSlider`, it follows the
/// pointer itself.
final class MenuWindPad: NSView {
    var changed: ((Wind) -> Void)?
    /// Dropped into the last stop, the wind wanders; dragged back out, it blows as set.
    var wanderChanged: ((Bool) -> Void)?

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

    private static let height: CGFloat = 156
    private static let top: CGFloat = 2
    private static let bottom: CGFloat = 8
    /// In line with the rows' titles on the left and their chevrons on the right.
    private static let inset: CGFloat = 14
    /// The handle's ring, which stays inside the field at full strength.
    private static let handle: CGFloat = 11
    /// The last stop's stretch, with the gap before it where the tick stands.
    private static let stop: CGFloat = 44
    private static let gap: CGFloat = 12

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: Self.top + Self.height + Self.bottom))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.slider)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    private var field: NSRect {
        NSRect(x: Self.inset, y: Self.top, width: bounds.width - 2 * Self.inset - Self.stop, height: Self.height)
    }

    private var tick: CGFloat { field.maxX + Self.gap / 2 }

    private var slot: NSPoint { NSPoint(x: bounds.width - Self.inset - (Self.stop - Self.gap) / 2, y: field.midY) }

    private var middle: NSPoint { NSPoint(x: field.midX, y: field.midY) }

    /// How far full strength reaches across and up and down.
    private var reach: (x: CGFloat, y: CGFloat) {
        let margin = Self.handle + 5
        return (field.width / 2 - margin, field.height / 2 - margin)
    }

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
    /// the eight compass points within a few degrees of one; or, past the tick, into the last stop.
    private func follow(_ event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard point.x < tick else {
            if !wanders {
                wanders = true
                wanderChanged?(true)
            }
            return
        }
        let x = point.x - middle.x
        let y = middle.y - point.y
        let strength = (min(max(abs(x) / reach.x, abs(y) / reach.y), 1) * 20).rounded() / 20
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
            wanderChanged?(false)
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
        let middle = middle
        let reach = reach
        let ink = NSColor.labelColor
        drawStop(ink)

        let well = NSBezierPath(roundedRect: field.insetBy(dx: 0.5, dy: 0.5), xRadius: 10, yRadius: 10)
        ink.withAlphaComponent(0.06).setFill()
        well.fill()
        (dragging && !wanders ? ink : NSColor.separatorColor).setStroke()
        well.lineWidth = 1
        well.stroke()

        ink.withAlphaComponent(0.3).setFill()
        let columns = max(1, Int((reach.x / (reach.y / 3)).rounded()))
        for i in -columns...columns {
            for j in -3...3 {
                let dot = NSPoint(x: middle.x + CGFloat(i) * reach.x / CGFloat(columns), y: middle.y + CGFloat(j) * reach.y / 3)
                NSBezierPath(ovalIn: NSRect(x: dot.x - 1.5, y: dot.y - 1.5, width: 3, height: 3)).fill()
            }
        }

        // Out along the wind's own direction to where the field puts its strength.
        let vector = wind.vector
        let share = max(abs(CGFloat(vector.x)) / reach.x, abs(CGFloat(vector.y)) / reach.y)
        let out = share > 0 ? CGFloat(wind.strength) / share : 0
        let tip = NSPoint(x: middle.x + CGFloat(vector.x) * out, y: middle.y - CGFloat(vector.y) * out)
        // Wandering, the handle rests in the last stop and the arrow only shows how the wind blows.
        let ring = Self.handle
        if !wanders {
            ink.withAlphaComponent(dragging ? 0.28 : 0.16).setFill()
            NSBezierPath(ovalIn: NSRect(x: tip.x - ring, y: tip.y - ring, width: ring * 2, height: ring * 2)).fill()
        }

        (wanders ? ink.withAlphaComponent(0.55) : ink).set()
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

    /// The tick and the last stop, a round well the handle rests in, named underneath.
    private func drawStop(_ ink: NSColor) {
        let slot = slot
        ink.withAlphaComponent(0.35).setFill()
        NSRect(x: (tick - 0.5).rounded(), y: slot.y - 12, width: 1, height: 24).fill()
        let radius = Self.handle + 4
        let well = NSBezierPath(ovalIn: NSRect(x: slot.x - radius + 0.5, y: slot.y - radius + 0.5, width: radius * 2 - 1, height: radius * 2 - 1))
        ink.withAlphaComponent(0.06).setFill()
        well.fill()
        (dragging && wanders ? ink : NSColor.separatorColor).setStroke()
        well.lineWidth = 1
        well.stroke()
        if wanders {
            let ring = Self.handle
            ink.withAlphaComponent(dragging ? 0.28 : 0.16).setFill()
            NSBezierPath(ovalIn: NSRect(x: slot.x - ring, y: slot.y - ring, width: ring * 2, height: ring * 2)).fill()
            ink.setFill()
            NSBezierPath(ovalIn: NSRect(x: slot.x - 3.5, y: slot.y - 3.5, width: 7, height: 7)).fill()
        }
        let caption = NSAttributedString(string: tr("variable"), attributes: [
            .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize - 1),
            .foregroundColor: wanders ? NSColor.labelColor : .tertiaryLabelColor,
        ])
        let size = caption.size()
        caption.draw(at: NSPoint(x: bounds.width - Self.inset - size.width, y: slot.y + radius + 4))
    }
}
