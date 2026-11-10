import AppKit

/// The intensity in steps of 5%, and past 100%, set apart by a tick, a last stop where the snowfall wanders on its own.
/// Like `MenuSlider`, the view follows the pointer itself.
final class MenuIntensitySlider: NSView {
    enum Value: Equatable {
        case steady(Double)
        case variable
    }

    var changed: ((Value) -> Void)?

    var value = Value.steady(0.5) {
        didSet { needsDisplay = true }
    }

    private var held = false

    // Measured off an NSSlider in a window that isn't key, as the menu's are: a 20 by 16 pt knob, white and a little
    // see-through in the dark, on a 6 pt track 23% opaque where filled and 10% beyond; set as Opacity's is, 16 pt in
    // from either end and 4 pt down.
    private static let inset: CGFloat = 16
    private static let knob = NSSize(width: 20, height: 16)
    private static let track: CGFloat = 6
    /// The last stop's stretch of track, with the gap before it where the tick stands.
    private static let stop: CGFloat = 44
    private static let gap: CGFloat = 12
    private static let trackMiddle: CGFloat = 12

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 38))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.slider)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    static func describe(_ value: Value) -> String {
        switch value {
        case .steady(let intensity): MenuSlider.percent(intensity)
        case .variable: tr("variable")
        }
    }

    private var mainEnd: CGFloat { bounds.width - Self.inset - Self.stop }

    private var start: CGFloat { Self.inset + Self.knob.width / 2 }

    private var length: CGFloat { max(1, mainEnd - Self.knob.width / 2 - start) }

    private var tick: CGFloat { mainEnd + Self.gap / 2 }

    private var variableX: CGFloat { bounds.width - Self.inset - Self.knob.width / 2 }

    private func x(_ value: Value) -> CGFloat {
        switch value {
        case .steady(let intensity): start + CGFloat(intensity) * length
        case .variable: variableX
        }
    }

    private func value(at x: CGFloat) -> Value {
        guard x < tick else { return .variable }
        let along = min(max(Double((x - start) / length), 0), 1)
        return .steady((along * 20).rounded() / 20)
    }

    override func mouseDown(with event: NSEvent) {
        held = true
        follow(event)
    }

    override func mouseDragged(with event: NSEvent) {
        follow(event)
    }

    override func mouseUp(with event: NSEvent) { release() }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        release()
    }

    private func release() {
        guard held else { return }
        held = false
        needsDisplay = true
    }

    private func follow(_ event: NSEvent) {
        needsDisplay = true
        set(value(at: convert(event.locationInWindow, from: nil).x))
    }

    private func set(_ next: Value) {
        guard next != value else { return }
        value = next
        changed?(next)
    }

    override func accessibilityLabel() -> String? { tr("intensity") }

    override func accessibilityValue() -> Any? { Self.describe(value) }

    override func accessibilityPerformIncrement() -> Bool {
        if case .steady(let intensity) = value { set(intensity >= 1 ? .variable : .steady(min(1, intensity + 0.05))) }
        return true
    }

    override func accessibilityPerformDecrement() -> Bool {
        switch value {
        case .steady(let intensity): set(.steady(max(0, intensity - 0.05)))
        case .variable: set(.steady(1))
        }
        return true
    }

    override func draw(_ dirtyRect: NSRect) {
        let middle = Self.trackMiddle
        let radius = Self.track / 2
        let main = NSRect(x: Self.inset, y: middle - radius, width: mainEnd - Self.inset, height: Self.track)
        let last = NSRect(x: mainEnd + Self.gap, y: main.minY, width: bounds.width - Self.inset - mainEnd - Self.gap, height: Self.track)
        let knobX = x(value)
        let variable = value == .variable

        drawTrack(main, filledTo: variable ? main.maxX : knobX)
        drawTrack(last, filledTo: variable ? knobX : last.minX)

        NSColor.labelColor.withAlphaComponent(0.35).setFill()
        NSRect(x: (tick - 0.5).rounded(), y: middle - 6, width: 1, height: 12).fill()

        let caption = NSAttributedString(string: tr("variable"), attributes: [
            .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize - 1),
            .foregroundColor: variable ? NSColor.labelColor : .tertiaryLabelColor,
        ])
        let size = caption.size()
        caption.draw(at: NSPoint(x: bounds.width - Self.inset - size.width, y: middle + Self.knob.height / 2 + 2))

        drawKnob(at: knobX, middle: middle)
    }

    /// Filled up to `x` and empty past it, the two never laid one over the other.
    private func drawTrack(_ track: NSRect, filledTo x: CGFloat) {
        let split = min(max(x, track.minX), track.maxX)
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(roundedRect: track, xRadius: track.height / 2, yRadius: track.height / 2).addClip()
        NSColor.labelColor.withAlphaComponent(0.23).setFill()
        NSRect(x: track.minX, y: track.minY, width: split - track.minX, height: track.height).fill()
        NSColor.labelColor.withAlphaComponent(0.1).setFill()
        NSRect(x: split, y: track.minY, width: track.maxX - split, height: track.height).fill()
        NSGraphicsContext.restoreGraphicsState()
    }

    private func drawKnob(at center: CGFloat, middle: CGFloat) {
        let size = Self.knob
        let rect = NSRect(x: center - size.width / 2, y: middle - size.height / 2, width: size.width, height: size.height)
        let shape = NSBezierPath(roundedRect: rect, xRadius: size.height / 2, yRadius: size.height / 2)
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.12)
        shadow.shadowBlurRadius = 1.5
        shadow.shadowOffset = NSSize(width: 0, height: -0.5)
        shadow.set()
        let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        NSColor.white.withAlphaComponent((dark ? 0.85 : 1) * (held ? 0.8 : 1)).setFill()
        shape.fill()
        NSGraphicsContext.restoreGraphicsState()
    }
}
