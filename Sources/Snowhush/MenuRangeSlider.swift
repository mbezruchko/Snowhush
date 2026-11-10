import AppKit

/// The intensity as a range, in steps of 5%. Its two knobs lie as one while the snowfall is steady; pulled apart, the
/// snowfall wanders between them. A knob dragged out of the pair goes the way it is dragged; the track moves the pair,
/// and the band between two knobs moves the whole range; double-clicked, the range closes up on its middle. Like
/// `MenuSlider`, the view follows the pointer itself.
final class MenuRangeSlider: NSView {
    var changed: ((ClosedRange<Double>) -> Void)?

    var range: ClosedRange<Double> = 0.5...0.5 {
        didSet { needsDisplay = true }
    }

    private enum Grip {
        /// Both knobs, lying as one: the first move decides which comes away.
        case pair
        case low
        case high
        /// The whole range, held this far above its bottom.
        case band(Double)
    }

    private var grip: Grip?

    // As the menu's own sliders draw them: a white capsule knob on a thin track, travelling as far in from the ends.
    private static let inset: CGFloat = 16
    private static let knob = NSSize(width: 26, height: 18)
    private static let track: CGFloat = 5

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 30))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.slider)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    /// The percentage, or the range's two ends.
    static func describe(_ range: ClosedRange<Double>) -> String {
        let low = Int((range.lowerBound * 100).rounded())
        let high = Int((range.upperBound * 100).rounded())
        return low == high ? MenuSlider.percent(range.lowerBound) : "\(low)–\(high)%"
    }

    private var start: CGFloat { Self.inset + Self.knob.width / 2 }

    private var length: CGFloat { max(1, bounds.width - 2 * start) }

    private func x(_ value: Double) -> CGFloat { start + CGFloat(value) * length }

    private func value(_ x: CGFloat) -> Double { min(max(Double((x - start) / length), 0), 1) }

    private static func step(_ value: Double) -> Double { (value * 20).rounded() / 20 }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil).x
        let low = range.lowerBound, high = range.upperBound
        if event.clickCount == 2 {
            let middle = Self.step((low + high) / 2)
            set(middle...middle)
            return
        }
        let half = Self.knob.width / 2
        let toLow = abs(point - x(low)), toHigh = abs(point - x(high))
        if low == high {
            grip = toLow <= half ? .pair : .band(0)
        } else if toHigh <= half && toHigh <= toLow {
            grip = .high
        } else if toLow <= half {
            grip = .low
        } else if point > x(low) && point < x(high) {
            grip = .band(value(point) - low)
            needsDisplay = true
            return
        } else {
            grip = point < x(low) ? .low : .high
        }
        needsDisplay = true
        if case .pair = grip { return }
        follow(point)
    }

    override func mouseDragged(with event: NSEvent) {
        follow(convert(event.locationInWindow, from: nil).x)
    }

    override func mouseUp(with event: NSEvent) { release() }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        release()
    }

    private func release() {
        guard grip != nil else { return }
        grip = nil
        needsDisplay = true
    }

    private func follow(_ point: CGFloat) {
        guard let grip else { return }
        let low = range.lowerBound, high = range.upperBound
        let at = Self.step(value(point))
        switch grip {
        case .pair:
            if at < low {
                self.grip = .low
            } else if at > high {
                self.grip = .high
            } else {
                return
            }
            follow(point)
        case .low:
            set(min(at, high)...high)
        case .high:
            set(low...max(at, low))
        case .band(let above):
            let width = high - low
            let bottom = min(max(Self.step(value(point) - above), 0), 1 - width)
            set(bottom...Self.step(bottom + width))
        }
    }

    private func set(_ next: ClosedRange<Double>) {
        guard next != range else { return }
        range = next
        changed?(next)
    }

    override func accessibilityLabel() -> String? { tr("intensity") }

    override func accessibilityValue() -> Any? { Self.describe(range) }

    override func accessibilityPerformIncrement() -> Bool {
        shift(by: 0.05)
        return true
    }

    override func accessibilityPerformDecrement() -> Bool {
        shift(by: -0.05)
        return true
    }

    private func shift(by amount: Double) {
        let width = range.upperBound - range.lowerBound
        let bottom = min(max(Self.step(range.lowerBound + amount), 0), 1 - width)
        set(bottom...Self.step(bottom + width))
    }

    override func draw(_ dirtyRect: NSRect) {
        let middle = bounds.height / 2 - 0.5
        let low = range.lowerBound, high = range.upperBound
        let track = NSRect(x: Self.inset, y: middle - Self.track / 2, width: bounds.width - 2 * Self.inset, height: Self.track)
        NSColor.labelColor.withAlphaComponent(0.12).setFill()
        NSBezierPath(roundedRect: track, xRadius: Self.track / 2, yRadius: Self.track / 2).fill()
        // Steady, it fills from the start as any slider does; a range fills only between its knobs.
        let from = low == high ? track.minX : x(low)
        let filled = NSRect(x: from, y: track.minY, width: max(0, x(high) - from), height: Self.track)
        NSColor.labelColor.withAlphaComponent(0.5).setFill()
        NSBezierPath(roundedRect: filled, xRadius: Self.track / 2, yRadius: Self.track / 2).fill()
        let held: (low: Bool, high: Bool) = switch grip {
        case .low: (true, false)
        case .high: (false, true)
        case .pair, .band: (true, true)
        case nil: (false, false)
        }
        drawKnob(at: x(low), middle: middle, held: held.low)
        if high != low { drawKnob(at: x(high), middle: middle, held: held.high) }
    }

    private func drawKnob(at center: CGFloat, middle: CGFloat, held: Bool) {
        let size = Self.knob
        let rect = NSRect(x: center - size.width / 2, y: middle - size.height / 2, width: size.width, height: size.height)
        let shape = NSBezierPath(roundedRect: rect, xRadius: size.height / 2, yRadius: size.height / 2)
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.3)
        shadow.shadowBlurRadius = 3
        shadow.shadowOffset = NSSize(width: 0, height: -1)
        shadow.set()
        NSColor.white.withAlphaComponent(held ? 0.82 : 1).setFill()
        shape.fill()
        NSGraphicsContext.restoreGraphicsState()
        NSColor.black.withAlphaComponent(0.06).setStroke()
        shape.lineWidth = 0.5
        shape.stroke()
    }
}
