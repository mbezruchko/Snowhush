import AppKit

/// A row of icons, each over its name, picked with a click: bright while on, faint while off, fainter still while it
/// can't be picked. The menu stays open. Every tile takes a column of the same width, `columns` of them across, so
/// rows with fewer tiles line up under a fuller one.
final class MenuTiles: NSView {
    struct Tile {
        let name: String
        let image: NSImage
    }

    var tiles: [Tile] = [] {
        didSet {
            hovered = nil
            needsDisplay = true
            setAccessibilityChildren(tiles.indices.map { k in
                let element = Element(k)
                element.setAccessibilityParent(self)
                return element
            })
        }
    }
    var columns = 4
    var on: [Bool] = [] {
        didSet { needsDisplay = true }
    }
    var enabled: [Bool] = [] {
        didSet { needsDisplay = true }
    }
    var toggled: (Int) -> Void = { _ in }
    private var hovered: Int?

    private static let height: CGFloat = 54
    private static let inset: CGFloat = 10
    private static let icon: CGFloat = 20

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: Self.height))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    /// Widens the row so the longest name fits its column: a menu sizes itself to its rows' views.
    func fit(_ names: [String]) {
        let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        let widest = names.map { NSAttributedString(string: $0, attributes: [.font: font]).size().width }.max() ?? 0
        setFrameSize(NSSize(width: max(200, ceil(2 * Self.inset + CGFloat(columns) * max(widest + 14, 52))), height: frame.height))
    }

    private func isOn(_ k: Int) -> Bool { k < on.count && on[k] }

    private func isEnabled(_ k: Int) -> Bool { k >= enabled.count || enabled[k] }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) { hover(event) }

    override func mouseEntered(with event: NSEvent) {
        hover(event)
        enclosingMenuItem?.menu?.items.forEach { $0.view?.needsDisplay = true }
    }

    override func mouseExited(with event: NSEvent) {
        hovered = nil
        needsDisplay = true
        enclosingMenuItem?.menu?.items.forEach { $0.view?.needsDisplay = true }
    }

    private func hover(_ event: NSEvent) {
        let next = tile(at: convert(event.locationInWindow, from: nil)).flatMap { isEnabled($0) ? $0 : nil }
        guard next != hovered else { return }
        hovered = next
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let k = tile(at: convert(event.locationInWindow, from: nil)), isEnabled(k) else { return }
        toggled(k)
    }

    private func tile(at point: NSPoint) -> Int? {
        guard !tiles.isEmpty, bounds.contains(point) else { return nil }
        let k = Int((point.x - Self.inset) / columnWidth)
        return k >= 0 && k < tiles.count ? k : nil
    }

    private var columnWidth: CGFloat { (bounds.width - 2 * Self.inset) / CGFloat(max(columns, tiles.count, 1)) }

    private func frame(of k: Int) -> NSRect {
        NSRect(x: Self.inset + columnWidth * CGFloat(k), y: 2, width: columnWidth, height: bounds.height - 4)
    }

    override func draw(_ dirtyRect: NSRect) {
        let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        for (k, tile) in tiles.enumerated() {
            let box = frame(of: k)
            if hovered == k {
                NSColor.quaternaryLabelColor.setFill()
                NSBezierPath(roundedRect: box.insetBy(dx: 2, dy: 0), xRadius: MenuMetrics.highlightRadius, yRadius: MenuMetrics.highlightRadius).fill()
            }
            let lit = isOn(k)
            let usable = isEnabled(k)
            let ink: NSColor = !usable ? .quaternaryLabelColor : lit ? .labelColor : .tertiaryLabelColor
            let size = tile.image.size
            let scale = min(1, Self.icon / max(size.width, size.height))
            let icon = NSRect(x: box.midX - size.width * scale / 2, y: box.minY + 6 + (Self.icon - size.height * scale) / 2,
                              width: size.width * scale, height: size.height * scale)
            tile.image.tinted(ink).draw(in: icon, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
            let name = NSAttributedString(string: tile.name, attributes: [
                .font: font,
                .foregroundColor: !usable ? NSColor.quaternaryLabelColor : lit ? .labelColor : .secondaryLabelColor,
            ])
            let text = name.size()
            name.draw(at: NSPoint(x: box.midX - text.width / 2, y: box.maxY - text.height - 4))
        }
    }

    /// One tile to accessibility: a checkbox named for what it picks.
    private final class Element: NSAccessibilityElement {
        let k: Int

        init(_ k: Int) {
            self.k = k
            super.init()
            setAccessibilityRole(.checkBox)
        }

        private var row: MenuTiles? { accessibilityParent() as? MenuTiles }

        override func accessibilityLabel() -> String? { row.flatMap { k < $0.tiles.count ? $0.tiles[k].name : nil } }

        override func accessibilityValue() -> Any? { row?.isOn(k) == true ? 1 : 0 }

        override func isAccessibilityEnabled() -> Bool { row?.isEnabled(k) ?? false }

        override func accessibilityFrame() -> NSRect {
            guard let row, let window = row.window else { return .zero }
            return window.convertToScreen(row.convert(row.frame(of: k), to: nil))
        }

        override func accessibilityPerformPress() -> Bool {
            guard let row, row.isEnabled(k) else { return false }
            row.toggled(k)
            return true
        }
    }
}

extension NSImage {
    /// The image in one colour: symbols and drawn template icons alike. A see-through colour fills once, not twice.
    func tinted(_ color: NSColor) -> NSImage {
        NSImage(size: size, flipped: false) { [self] rect in
            draw(in: rect)
            color.set()
            rect.fill(using: .sourceIn)
            return true
        }
    }
}
