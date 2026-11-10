import AppKit

/// A menu row with a switch at its right end, as the system's own menu bar menus have them: a click anywhere on the
/// row flips it, and the menu stays open. It shows its enclosing item's title.
///
/// The switch is drawn here: an NSSwitch in a menu, a window that is never key, tells on from off only by a track 15%
/// opaque instead of 10%. This one fills with the accent colour while on, at an NSSwitch's size, without the padding
/// an NSSwitch keeps around itself.
final class MenuSwitch: NSView {
    var toggled: (Bool) -> Void = { _ in }

    var isOn = false {
        didSet {
            guard isOn != oldValue else { return }
            slide = isOn ? 1 : 0
            stopSliding()
            needsDisplay = true
        }
    }

    var isEnabled = true {
        didSet { needsDisplay = true }
    }

    /// Where the knob stands, from 0 at the left to 1 at the right; it slides across when flipped by hand.
    private var slide: CGFloat = 0
    private var sliding: Timer?

    private static let track = NSSize(width: 36, height: 16)
    private static let knob = NSSize(width: 22, height: 14)
    private static let trailing: CGFloat = 15
    private static let gap: CGFloat = 18

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: MenuMetrics.rowHeight + 4))
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

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        flip()
    }

    override func accessibilityPerformPress() -> Bool {
        flip()
        return true
    }

    override func accessibilityLabel() -> String? { enclosingMenuItem?.title }

    override func accessibilityValue() -> Any? { isOn ? 1 : 0 }

    override func isAccessibilityEnabled() -> Bool { isEnabled }

    private func flip() {
        guard isEnabled else { return }
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
            let eased = 1 - pow(1 - t, 3)
            slide = start + (target - start) * eased
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

    /// Widens the row to fit `title` and the switch: a menu sizes itself to its rows' views.
    func fit(_ title: String) {
        let text = NSAttributedString(string: title, attributes: [.font: NSFont.menuFont(ofSize: 0)])
        let width = MenuDisclosure.titleX + text.size().width + Self.gap + Self.track.width + Self.trailing
        setFrameSize(NSSize(width: max(200, ceil(width)), height: frame.height))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let item = enclosingMenuItem else { return }
        let color: NSColor = isEnabled ? .labelColor : .disabledControlTextColor
        let title = NSAttributedString(string: item.title, attributes: [.font: NSFont.menuFont(ofSize: 0), .foregroundColor: color])
        title.draw(at: NSPoint(x: MenuDisclosure.titleX, y: (bounds.height - title.size().height) / 2))
        drawSwitch()
    }

    private func drawSwitch() {
        let size = Self.track
        let track = NSRect(x: bounds.width - Self.trailing - size.width, y: ((bounds.height - size.height) / 2).rounded(), width: size.width, height: size.height)
        let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        NSGraphicsContext.saveGraphicsState()
        if !isEnabled { NSGraphicsContext.current?.cgContext.setAlpha(0.4) }
        let shape = NSBezierPath(roundedRect: track, xRadius: size.height / 2, yRadius: size.height / 2)
        // The accent comes in as the knob slides over, so a flip by hand fades rather than snaps; the grey goes as it
        // does, or its rim would show around the accent.
        NSColor.labelColor.withAlphaComponent(0.1 * (1 - slide)).setFill()
        shape.fill()
        NSColor.controlAccentColor.withAlphaComponent(slide).setFill()
        shape.fill()

        let knob = Self.knob
        let inset = (size.height - knob.height) / 2
        let x = track.minX + inset + (size.width - knob.width - 2 * inset) * slide
        let knobRect = NSRect(x: x, y: track.minY + inset, width: knob.width, height: knob.height)
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.2)
        shadow.shadowBlurRadius = 1.5
        shadow.shadowOffset = NSSize(width: 0, height: -0.5)
        shadow.set()
        // White on the accent; a little see-through in the dark while off, as an NSSwitch's knob is.
        NSColor.white.withAlphaComponent(dark ? 0.87 + 0.13 * slide : 1).setFill()
        NSBezierPath(roundedRect: knobRect, xRadius: knob.height / 2, yRadius: knob.height / 2).fill()
        NSGraphicsContext.restoreGraphicsState()
    }
}
