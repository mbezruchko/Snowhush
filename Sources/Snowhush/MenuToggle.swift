import AppKit

/// A checkmark menu row that keeps its menu open when clicked, so several options can be changed in one go.
/// It draws its enclosing item's title, state and enabled flag, so the menu's delegate updates it like any other item.
final class MenuToggle: NSView {
    private let action: Selector
    private weak var target: AnyObject?

    // Measured from a macOS 26 menu at 2x: rows 24 pt, titles and separators at 30 pt, checkmarks centred at 18 pt
    // and matching a 10.5 pt bold SF checkmark, not the older, thinner menuOnStateTemplate; lit in the accent colour.
    private static let modern = ProcessInfo.processInfo.isOperatingSystemAtLeast(OperatingSystemVersion(majorVersion: 26, minorVersion: 0, patchVersion: 0))
    private static let rowHeight: CGFloat = modern ? 24 : 22
    private static let titleX: CGFloat = modern ? 30 : 22
    private static let checkX: CGFloat = modern ? 18 : 12
    private static let highlightRadius: CGFloat = modern ? 7 : 4
    private static let check = NSImage(systemSymbolName: "checkmark", accessibilityDescription: nil)

    init(action: Selector, target: AnyObject) {
        self.action = action
        self.target = target
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: Self.rowHeight))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.checkBox)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
    }

    // The menu moves its highlight without always telling a row the pointer left, so every row redraws from the
    // item the menu has lit.
    override func mouseEntered(with event: NSEvent) { redrawRows() }

    override func mouseExited(with event: NSEvent) { redrawRows() }

    private func redrawRows() {
        enclosingMenuItem?.menu?.items.forEach { $0.view?.needsDisplay = true }
    }

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        press()
    }

    override func accessibilityPerformPress() -> Bool {
        press()
        return true
    }

    override func accessibilityLabel() -> String? { enclosingMenuItem?.title }

    override func accessibilityValue() -> Any? { enclosingMenuItem?.state == .on ? 1 : 0 }

    private func press() {
        guard let item = enclosingMenuItem, item.isEnabled else { return }
        NSApp.sendAction(action, to: target, from: item)
        var root = item.menu
        while let parent = root?.supermenu { root = parent }
        if let root { root.delegate?.menuNeedsUpdate?(root) }
        redrawRows()
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let item = enclosingMenuItem else { return }
        let lit = item.isEnabled && item.isHighlighted
        if lit {
            NSColor.controlAccentColor.setFill()
            NSBezierPath(roundedRect: bounds.insetBy(dx: 5, dy: 0), xRadius: Self.highlightRadius, yRadius: Self.highlightRadius).fill()
        }
        let color: NSColor = lit ? .selectedMenuItemTextColor : item.isEnabled ? .labelColor : .disabledControlTextColor
        if item.state == .on, let check = Self.check?.withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 10.5, weight: .bold).applying(.init(paletteColors: [color]))) {
            check.draw(in: NSRect(x: Self.checkX - check.size.width / 2, y: (bounds.height - check.size.height) / 2, width: check.size.width, height: check.size.height))
        }
        let title = NSAttributedString(string: item.title, attributes: [.font: NSFont.menuFont(ofSize: 0), .foregroundColor: color])
        title.draw(at: NSPoint(x: Self.titleX, y: (bounds.height - title.size().height) / 2))
    }
}
