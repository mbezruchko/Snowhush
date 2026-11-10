import AppKit

/// The menu's first row, as the system's own menu bar modules head theirs: the app's name in semibold and, at the
/// right end, a switch that stops the weather and starts it again. Only the switch answers a click; the menu stays
/// open.
///
/// The switch is drawn here: an NSSwitch in a menu, a window that is never key, tells on from off only by a track 15%
/// opaque instead of 10%.
final class MenuHeader: NSView {
    var toggled: (Bool) -> Void = { _ in }

    var isOn = false {
        didSet {
            guard isOn != oldValue else { return }
            slide = isOn ? 1 : 0
            stopSliding()
            needsDisplay = true
        }
    }

    private let title: String
    private var slide: CGFloat = 0
    private var sliding: Timer?

    private static let track = NSSize(width: 36, height: 16)
    private static let knob = NSSize(width: 22, height: 14)
    private static let trailing: CGFloat = 14

    init(title: String) {
        self.title = title
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 34))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.checkBox)
        setAccessibilitySubrole(.switch)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    private var trackFrame: NSRect {
        let size = Self.track
        return NSRect(x: bounds.width - Self.trailing - size.width, y: ((bounds.height - size.height) / 2).rounded(), width: size.width, height: size.height)
    }

    override func mouseUp(with event: NSEvent) {
        guard trackFrame.insetBy(dx: -8, dy: -8).contains(convert(event.locationInWindow, from: nil)) else { return }
        flip()
    }

    override func accessibilityPerformPress() -> Bool {
        flip()
        return true
    }

    override func accessibilityLabel() -> String? { title }

    override func accessibilityValue() -> Any? { isOn ? 1 : 0 }

    private func flip() {
        let from = slide
        isOn.toggle()
        slide = from
        slideAcross()
        toggled(isOn)
    }

    /// Eases the knob over in a fifth of a second, on a timer that runs while the menu tracks the pointer too.
    private func slideAcross() {
        stopSliding()
        let start = slide
        let target: CGFloat = isOn ? 1 : 0
        let began = CACurrentMediaTime()
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] timer in
            guard let self else { return timer.invalidate() }
            let t = min(1, (CACurrentMediaTime() - began) / 0.2)
            slide = start + (target - start) * (1 - pow(1 - t, 3))
            needsDisplay = true
            if t >= 1 { stopSliding() }
        }
        RunLoop.main.add(timer, forMode: .common)
        sliding = timer
    }

    private func stopSliding() {
        sliding?.invalidate()
        sliding = nil
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        stopSliding()
        slide = isOn ? 1 : 0
    }

    override func draw(_ dirtyRect: NSRect) {
        let font = NSFont.systemFont(ofSize: NSFont.menuFont(ofSize: 0).pointSize, weight: .semibold)
        let name = NSAttributedString(string: title, attributes: [.font: font, .foregroundColor: NSColor.labelColor])
        name.draw(at: NSPoint(x: MenuDisclosure.titleX, y: (bounds.height - name.size().height) / 2))

        let track = trackFrame
        let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        NSGraphicsContext.saveGraphicsState()
        let shape = NSBezierPath(roundedRect: track, xRadius: track.height / 2, yRadius: track.height / 2)
        // The accent comes in as the knob slides over; the grey goes as it does, or its rim would show around it.
        NSColor.labelColor.withAlphaComponent(0.12 * (1 - slide)).setFill()
        shape.fill()
        NSColor.controlAccentColor.withAlphaComponent(slide).setFill()
        shape.fill()
        let knob = Self.knob
        let inset = (track.height - knob.height) / 2
        let x = track.minX + inset + (track.width - knob.width - 2 * inset) * slide
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.2)
        shadow.shadowBlurRadius = 1.5
        shadow.shadowOffset = NSSize(width: 0, height: -0.5)
        shadow.set()
        NSColor.white.withAlphaComponent(dark ? 0.87 + 0.13 * slide : 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: x, y: track.minY + inset, width: knob.width, height: knob.height), xRadius: knob.height / 2, yRadius: knob.height / 2).fill()
        NSGraphicsContext.restoreGraphicsState()
    }
}
