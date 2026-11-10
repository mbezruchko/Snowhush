import AppKit

/// The menu's tiles, two to a row as Control Center's are: each a round icon, a name and what it is set to, and a
/// click opens its page in the menu's place. The menu stays open.
final class MenuCards: NSView {
    struct Card {
        var title: String
        var detail: String
        var image: NSImage?
        /// The icon stands on the accent colour: the tile is on something other than its default.
        var lit = false
    }

    var cards: [Card] = [] {
        didSet {
            needsDisplay = true
            guard cards.count != oldValue.count else { return }
            setFrameSize(NSSize(width: frame.width, height: height))
            setAccessibilityChildren(cards.indices.map { k in
                let element = Element(k)
                element.setAccessibilityParent(self)
                return element
            })
        }
    }
    var pressed: (Int) -> Void = { _ in }
    private var hovered: Int?

    private static let inset: CGFloat = 12
    private static let gap: CGFloat = 8
    private static let tile: CGFloat = 50
    private static let circle: CGFloat = 30

    private var height: CGFloat {
        let rows = CGFloat((cards.count + 1) / 2)
        return 4 + rows * Self.tile + max(0, rows - 1) * Self.gap + 8
    }

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 300, height: 120))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    fileprivate func frame(of k: Int) -> NSRect {
        let width = (bounds.width - 2 * Self.inset - Self.gap) / 2
        return NSRect(x: Self.inset + CGFloat(k % 2) * (width + Self.gap), y: 4 + CGFloat(k / 2) * (Self.tile + Self.gap), width: width, height: Self.tile)
    }

    private func card(at point: NSPoint) -> Int? {
        cards.indices.first { frame(of: $0).contains(point) }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) { hover(card(at: convert(event.locationInWindow, from: nil))) }

    override func mouseEntered(with event: NSEvent) { hover(card(at: convert(event.locationInWindow, from: nil))) }

    override func mouseExited(with event: NSEvent) { hover(nil) }

    private func hover(_ next: Int?) {
        guard next != hovered else { return }
        hovered = next
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let k = card(at: convert(event.locationInWindow, from: nil)) else { return }
        hover(nil)
        pressed(k)
    }

    override func draw(_ dirtyRect: NSRect) {
        let titleFont = NSFont.menuFont(ofSize: 0)
        let detailFont = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        let breaks = NSMutableParagraphStyle()
        breaks.lineBreakMode = .byTruncatingTail
        for (k, card) in cards.enumerated() {
            let box = frame(of: k)
            NSColor.labelColor.withAlphaComponent(hovered == k ? 0.14 : 0.08).setFill()
            NSBezierPath(roundedRect: box, xRadius: 12, yRadius: 12).fill()
            let circle = NSRect(x: box.minX + 10, y: box.midY - Self.circle / 2, width: Self.circle, height: Self.circle)
            (card.lit ? NSColor.controlAccentColor : NSColor.labelColor.withAlphaComponent(0.12)).setFill()
            NSBezierPath(ovalIn: circle).fill()
            if let image = card.image {
                let scale = min(1, 17 / max(image.size.width, image.size.height))
                let size = NSSize(width: image.size.width * scale, height: image.size.height * scale)
                let spot = NSRect(x: circle.midX - size.width / 2, y: circle.midY - size.height / 2, width: size.width, height: size.height)
                image.tinted(card.lit ? .white : .labelColor).draw(in: spot, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
            }
            let title = NSAttributedString(string: card.title, attributes: [.font: titleFont, .foregroundColor: NSColor.labelColor, .paragraphStyle: breaks])
            let detail = NSAttributedString(string: card.detail, attributes: [.font: detailFont, .foregroundColor: NSColor.secondaryLabelColor, .paragraphStyle: breaks])
            let x = circle.maxX + 8
            let width = box.maxX - 8 - x
            let lines = title.size().height + detail.size().height
            let top = box.midY - lines / 2
            title.draw(in: NSRect(x: x, y: top, width: width, height: title.size().height))
            detail.draw(in: NSRect(x: x, y: top + title.size().height, width: width, height: detail.size().height))
        }
    }

    private final class Element: NSAccessibilityElement {
        let k: Int

        init(_ k: Int) {
            self.k = k
            super.init()
            setAccessibilityRole(.button)
        }

        private var grid: MenuCards? { accessibilityParent() as? MenuCards }

        override func accessibilityLabel() -> String? { grid.flatMap { k < $0.cards.count ? $0.cards[k].title : nil } }

        override func accessibilityValue() -> Any? { grid.flatMap { k < $0.cards.count ? $0.cards[k].detail : nil } }

        override func accessibilityFrame() -> NSRect {
            guard let grid, let window = grid.window else { return .zero }
            return window.convertToScreen(grid.convert(grid.frame(of: k), to: nil))
        }

        override func accessibilityPerformPress() -> Bool {
            grid?.pressed(k)
            return grid != nil
        }
    }
}

/// The first row of a tile's page: a chevron and the page's name, and a click on them goes back to the tiles. While
/// `action` is set, a small button at the right end, as Reset is, answers on its own.
final class MenuBack: NSView {
    var back: () -> Void = {}
    var action: String? {
        didSet { needsDisplay = true }
    }
    var actionPressed: () -> Void = {}

    private let title: String
    private var actionFrame: NSRect = .zero
    private var hovered = false

    init(title: String) {
        self.title = title
        // As wide as the tiles, so the menu keeps its width from page to page.
        super.init(frame: NSRect(x: 0, y: 0, width: 300, height: 34))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func accessibilityLabel() -> String? { title }

    override func accessibilityPerformPress() -> Bool {
        back()
        return true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseEntered(with event: NSEvent) {
        hovered = true
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard bounds.contains(point) else { return }
        if action != nil, actionFrame.insetBy(dx: -4, dy: -6).contains(point) {
            actionPressed()
        } else {
            hovered = false
            back()
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        let font = NSFont.systemFont(ofSize: NSFont.menuFont(ofSize: 0).pointSize, weight: .semibold)
        var x = MenuDisclosure.titleX
        if let chevron = NSImage(systemSymbolName: "chevron.left", accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 12, weight: .semibold).applying(.init(paletteColors: [hovered ? .labelColor : .secondaryLabelColor]))) {
            chevron.draw(in: NSRect(x: x, y: (bounds.height - chevron.size.height) / 2, width: chevron.size.width, height: chevron.size.height))
            x += chevron.size.width + 7
        }
        let name = NSAttributedString(string: title, attributes: [.font: font, .foregroundColor: NSColor.labelColor])
        name.draw(at: NSPoint(x: x, y: (bounds.height - name.size().height) / 2))

        guard let action else {
            actionFrame = .zero
            return
        }
        let small = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
        let label = NSAttributedString(string: action, attributes: [.font: small, .foregroundColor: NSColor.secondaryLabelColor])
        let size = label.size()
        let height = ceil(size.height) + 4
        actionFrame = NSRect(x: bounds.width - 14 - ceil(size.width) - 18, y: (bounds.height - height) / 2, width: ceil(size.width) + 18, height: height)
        NSColor.labelColor.withAlphaComponent(0.08).setFill()
        NSBezierPath(roundedRect: actionFrame, xRadius: height / 2, yRadius: height / 2).fill()
        label.draw(at: NSPoint(x: actionFrame.midX - size.width / 2, y: actionFrame.midY - size.height / 2))
    }
}
