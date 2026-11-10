import AppKit

/// The modes as round buttons, each over its name, as Control Center's toggles are: filled with the accent colour
/// while on. The menu stays open. `compact`, the buttons are small with their names beside them, for what goes with
/// the modes: a second, quieter row.
final class MenuOrbs: NSView {
    struct Orb {
        let name: String
        let image: NSImage
    }

    var orbs: [Orb] = [] {
        didSet {
            needsDisplay = true
            setAccessibilityChildren(orbs.indices.map { k in
                let element = Element(k)
                element.setAccessibilityParent(self)
                return element
            })
        }
    }
    var on: [Bool] = [] {
        didSet { needsDisplay = true }
    }
    /// Icons drawn in a colour of their own while off, as More is in the accent while something under it is on.
    var tints: [Int: NSColor] = [:] {
        didSet { needsDisplay = true }
    }
    var toggled: (Int) -> Void = { _ in }
    /// How many columns the row is ruled into, when more than it has buttons: a shorter row then lines up under a
    /// fuller one.
    var columns = 0
    private var hovered: Int?

    private let compact: Bool
    private static let inset: CGFloat = 15
    private var circle: CGFloat { compact ? 26 : 40 }

    init(compact: Bool = false) {
        self.compact = compact
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: compact ? 38 : 70))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    /// Widens the row so the longest name fits its column: a menu sizes itself to its rows' views.
    func fit() {
        let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        let widest = orbs.map { NSAttributedString(string: $0.name, attributes: [.font: font]).size().width }.max() ?? 0
        let column = compact ? circle + 7 + widest + 14 : max(widest + 12, 60)
        setFrameSize(NSSize(width: max(200, ceil(2 * Self.inset + CGFloat(max(orbs.count, columns)) * column)), height: frame.height))
    }

    private func isOn(_ k: Int) -> Bool { k < on.count && on[k] }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) { hover(orb(at: convert(event.locationInWindow, from: nil))) }

    override func mouseEntered(with event: NSEvent) { hover(orb(at: convert(event.locationInWindow, from: nil))) }

    override func mouseExited(with event: NSEvent) { hover(nil) }

    private func hover(_ next: Int?) {
        guard next != hovered else { return }
        hovered = next
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let k = orb(at: convert(event.locationInWindow, from: nil)) else { return }
        toggled(k)
    }

    private var column: CGFloat { (bounds.width - 2 * Self.inset) / CGFloat(max(orbs.count, columns, 1)) }

    private func orb(at point: NSPoint) -> Int? {
        guard !orbs.isEmpty, bounds.contains(point) else { return nil }
        let k = Int((point.x - Self.inset) / column)
        return k >= 0 && k < orbs.count ? k : nil
    }

    private func frame(of k: Int) -> NSRect {
        NSRect(x: Self.inset + column * CGFloat(k), y: 0, width: column, height: bounds.height)
    }

    override func draw(_ dirtyRect: NSRect) {
        let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        for (k, orb) in orbs.enumerated() {
            let box = frame(of: k)
            let lit = isOn(k)
            let side = self.circle
            let circle = compact ? NSRect(x: box.minX, y: (bounds.height - side) / 2 - 1, width: side, height: side)
                : NSRect(x: box.midX - side / 2, y: 4, width: side, height: side)
            (lit ? NSColor.controlAccentColor : NSColor.labelColor.withAlphaComponent(hovered == k ? 0.2 : 0.12)).setFill()
            NSBezierPath(ovalIn: circle).fill()
            let scale = compact ? min(1, 15 / max(orb.image.size.width, orb.image.size.height)) : 1
            let size = NSSize(width: orb.image.size.width * scale, height: orb.image.size.height * scale)
            let icon = NSRect(x: circle.midX - size.width / 2, y: circle.midY - size.height / 2, width: size.width, height: size.height)
            orb.image.tinted(lit ? .white : tints[k] ?? .labelColor).draw(in: icon, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
            let name = NSAttributedString(string: orb.name, attributes: [.font: font, .foregroundColor: lit ? NSColor.labelColor : .secondaryLabelColor])
            let text = name.size()
            name.draw(at: compact ? NSPoint(x: circle.maxX + 7, y: circle.midY - text.height / 2) : NSPoint(x: box.midX - text.width / 2, y: circle.maxY + 6))
        }
    }

    /// One button to accessibility: a checkbox named for its mode.
    private final class Element: NSAccessibilityElement {
        let k: Int

        init(_ k: Int) {
            self.k = k
            super.init()
            setAccessibilityRole(.checkBox)
        }

        private var row: MenuOrbs? { accessibilityParent() as? MenuOrbs }

        override func accessibilityLabel() -> String? { row.flatMap { k < $0.orbs.count ? $0.orbs[k].name : nil } }

        override func accessibilityValue() -> Any? { row?.isOn(k) == true ? 1 : 0 }

        override func accessibilityFrame() -> NSRect {
            guard let row, let window = row.window else { return .zero }
            return window.convertToScreen(row.convert(row.frame(of: k), to: nil))
        }

        override func accessibilityPerformPress() -> Bool {
            row?.toggled(k)
            return row != nil
        }
    }
}

/// The intensity as a thick capsule that fills from the left, a snowflake at its start, as Control Center's Sound and
/// Display sliders are; in steps of 5%. Like `MenuSlider`, the view follows the pointer itself.
final class MenuBigSlider: NSView {
    var changed: ((Double) -> Void)?

    var value = 0.5 {
        didSet { needsDisplay = true }
    }
    /// Shown fainter while the snowfall wanders on its own and the slider only marks its middle.
    var dimmed = false {
        didSet { needsDisplay = true }
    }

    private static let inset: CGFloat = 15
    private static let thickness: CGFloat = 24
    private static let glyph = NSImage(systemSymbolName: "snowflake", accessibilityDescription: nil)?
        .withSymbolConfiguration(.init(pointSize: 11, weight: .semibold))

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 36))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.slider)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    private var capsule: NSRect {
        NSRect(x: Self.inset, y: 4, width: bounds.width - 2 * Self.inset, height: Self.thickness)
    }

    override func mouseDown(with event: NSEvent) { follow(event) }

    override func mouseDragged(with event: NSEvent) { follow(event) }

    private func follow(_ event: NSEvent) {
        let x = convert(event.locationInWindow, from: nil).x
        let run = max(1, capsule.width - Self.thickness)
        set((Double(min(max((x - capsule.minX - Self.thickness / 2) / run, 0), 1)) * 20).rounded() / 20)
    }

    private func set(_ next: Double) {
        guard next != value || dimmed else { return }
        value = next
        changed?(next)
    }

    override func accessibilityLabel() -> String? { tr("intensity") }

    override func accessibilityValue() -> Any? { MenuSlider.percent(value) }

    override func accessibilityPerformIncrement() -> Bool {
        set(min(1, value + 0.05))
        return true
    }

    override func accessibilityPerformDecrement() -> Bool {
        set(max(0, value - 0.05))
        return true
    }

    override func draw(_ dirtyRect: NSRect) {
        let capsule = capsule
        let radius = Self.thickness / 2
        NSColor.labelColor.withAlphaComponent(0.12).setFill()
        NSBezierPath(roundedRect: capsule, xRadius: radius, yRadius: radius).fill()
        let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        let filled = NSRect(x: capsule.minX, y: capsule.minY, width: Self.thickness + CGFloat(value) * (capsule.width - Self.thickness), height: capsule.height)
        NSGraphicsContext.saveGraphicsState()
        if !dark {
            let shadow = NSShadow()
            shadow.shadowColor = NSColor.black.withAlphaComponent(0.18)
            shadow.shadowBlurRadius = 2
            shadow.shadowOffset = NSSize(width: 0, height: -0.5)
            shadow.set()
        }
        NSColor.white.withAlphaComponent((dark ? 0.88 : 1) * (dimmed ? 0.5 : 1)).setFill()
        NSBezierPath(roundedRect: filled, xRadius: radius, yRadius: radius).fill()
        NSGraphicsContext.restoreGraphicsState()
        if let glyph = Self.glyph {
            let size = glyph.size
            let box = NSRect(x: capsule.minX + radius - size.width / 2, y: capsule.midY - size.height / 2, width: size.width, height: size.height)
            glyph.tinted(NSColor.black.withAlphaComponent(0.55)).draw(in: box, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        }
    }
}
