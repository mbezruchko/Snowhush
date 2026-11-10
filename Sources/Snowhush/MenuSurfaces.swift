import AppKit

/// A menu row that folds the row under it out or away, keeping the menu open. Folded, it sums up that row on its right.
final class MenuDisclosure: NSView {
    var open = false {
        didSet { needsDisplay = true }
    }
    var summary = "" {
        didSet { needsDisplay = true }
    }
    /// Sums the row up as these surfaces' icons instead, which take less room than their names.
    var summaryIcons: [Surfaces] = [] {
        didSet { needsDisplay = true }
    }
    var toggled: () -> Void = {}
    /// Shows the summary unfolded too, as a value the folded row sets.
    var summaryWhileOpen = false
    /// A small button the unfolded row shows, left of its summary or in its place, lit while `pillOn`.
    var pill: String? {
        didSet { needsDisplay = true }
    }
    var pillOn = false {
        didSet { needsDisplay = true }
    }
    var pillToggled: () -> Void = {}
    private var pillFrame: NSRect = .zero

    /// Where a title starts in a menu with no checkmarks, as the menu bar menu's own rows have it.
    static let titleX: CGFloat = MenuToggle.modern ? 15 : 14
    private static let summaryIcon: CGFloat = 15
    private static let summaryGap: CGFloat = 5

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: MenuToggle.rowHeight))
        autoresizingMask = .width
        setAccessibilityElement(true)
        setAccessibilityRole(.disclosureTriangle)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self))
    }

    override func mouseEntered(with event: NSEvent) { redrawRows() }

    override func mouseExited(with event: NSEvent) { redrawRows() }

    private func redrawRows() {
        enclosingMenuItem?.menu?.items.forEach { $0.view?.needsDisplay = true }
    }

    override func mouseUp(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard bounds.contains(point) else { return }
        if open, pill != nil, pillFrame.insetBy(dx: -4, dy: -3).contains(point) {
            pillToggled()
        } else {
            toggled()
        }
        redrawRows()
    }

    override func accessibilityPerformPress() -> Bool {
        toggled()
        return true
    }

    override func accessibilityLabel() -> String? { enclosingMenuItem?.title }

    override func accessibilityValue() -> Any? { open ? 1 : 0 }

    override func accessibilityCustomActions() -> [NSAccessibilityCustomAction]? {
        guard open, let pill else { return nil }
        return [NSAccessibilityCustomAction(name: pill) { [weak self] in
            self?.pillToggled()
            return true
        }]
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let item = enclosingMenuItem else { return }
        let lit = item.isHighlighted
        if lit {
            NSColor.controlAccentColor.setFill()
            let radius = MenuToggle.highlightRadius
            NSBezierPath(roundedRect: bounds.insetBy(dx: 5, dy: 0), xRadius: radius, yRadius: radius).fill()
        }
        let color: NSColor = lit ? .selectedMenuItemTextColor : .labelColor
        let soft: NSColor = lit ? .selectedMenuItemTextColor : .secondaryLabelColor
        let font = NSFont.menuFont(ofSize: 0)
        let title = NSAttributedString(string: item.title, attributes: [.font: font, .foregroundColor: color])
        title.draw(at: NSPoint(x: Self.titleX, y: (bounds.height - title.size().height) / 2))
        var right = bounds.width - 14
        let symbol = open ? "chevron.down" : "chevron.right"
        if let chevron = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 13, weight: .semibold).applying(.init(paletteColors: [color]))) {
            let size = chevron.size
            right -= 12
            chevron.draw(in: NSRect(x: right + (12 - size.width) / 2, y: (bounds.height - size.height) / 2, width: size.width, height: size.height))
            right -= 8
        }
        if open && !summaryWhileOpen {
            if let pill { drawPill(pill, right: right, lit: lit) }
            return
        }
        if !summaryIcons.isEmpty {
            let side = Self.summaryIcon
            for surface in summaryIcons.reversed() {
                right -= side
                SurfacePicker.drawIcon(surface, in: NSRect(x: right, y: (bounds.height - side) / 2, width: side, height: side), color: soft, pointSize: 12)
                right -= Self.summaryGap
            }
        } else if !summary.isEmpty {
            let trailing = NSAttributedString(string: summary, attributes: [.font: font, .foregroundColor: soft])
            let x = max(right - trailing.size().width, Self.titleX + title.size().width + 12)
            trailing.draw(at: NSPoint(x: x, y: (bounds.height - trailing.size().height) / 2))
            right = x - 8
        }
        if open, let pill { drawPill(pill, right: right, lit: lit) }
    }

    private func drawPill(_ text: String, right: CGFloat, lit: Bool) {
        let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
        let ink: NSColor = pillOn ? (lit ? .controlAccentColor : .selectedMenuItemTextColor) : lit ? .selectedMenuItemTextColor : .secondaryLabelColor
        let title = NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: ink])
        let size = title.size()
        let height = ceil(size.height) + 4
        pillFrame = NSRect(x: right - ceil(size.width) - 18, y: (bounds.height - height) / 2, width: ceil(size.width) + 18, height: height)
        let fill: NSColor = pillOn ? (lit ? .selectedMenuItemTextColor : .controlAccentColor)
            : lit ? NSColor.selectedMenuItemTextColor.withAlphaComponent(0.22) : NSColor.labelColor.withAlphaComponent(0.08)
        fill.setFill()
        NSBezierPath(roundedRect: pillFrame, xRadius: height / 2, yRadius: height / 2).fill()
        title.draw(at: NSPoint(x: pillFrame.midX - size.width / 2, y: pillFrame.midY - size.height / 2))
    }

    /// Widens the row to fit its title and summary, and to `least`, the row it folds, so the menu keeps its width as
    /// that row folds out: a menu sizes itself to its rows' views.
    func fit(_ title: String, least: CGFloat) {
        let font = NSFont.menuFont(ofSize: 0)
        let words = summaryIcons.isEmpty ? [title, summary] : [title]
        let icons = summaryIcons.isEmpty ? 0 : CGFloat(summaryIcons.count) * (Self.summaryIcon + Self.summaryGap)
        let width = words.map { NSAttributedString(string: $0, attributes: [.font: font]).size().width }.reduce(icons, +)
        setFrameSize(NSSize(width: max(200, least, ceil(Self.titleX + width + 56)), height: frame.height))
    }
}

/// A row of icons, each over its name, that switch what snow settles on; bright while on, faint while off. The menu
/// stays open, so several can be switched in one go.
final class SurfacePicker: NSView {
    /// What can be switched, in the order shown.
    var choices: [Surfaces] = [] {
        didSet {
            hovered = nil
            needsDisplay = true
            let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
            let widest = choices.map { NSAttributedString(string: Self.name(of: $0), attributes: [.font: font]).size().width }.max() ?? 0
            setFrameSize(NSSize(width: max(200, ceil(2 * Self.inset + CGFloat(choices.count) * max(widest + 14, 52))), height: frame.height))
            setAccessibilityChildren(choices.map { surface in
                let tile = Tile(surface)
                tile.setAccessibilityParent(self)
                return tile
            })
        }
    }
    var on: Surfaces = [] {
        didSet { needsDisplay = true }
    }
    /// Shown faint and left alone: a selection gathers no snow while windows don't.
    var disabled: Surfaces = [] {
        didSet { needsDisplay = true }
    }
    var toggled: (Surfaces) -> Void = { _ in }
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
        let next = tile(at: convert(event.locationInWindow, from: nil)).flatMap { disabled.contains(choices[$0]) ? nil : $0 }
        guard next != hovered else { return }
        hovered = next
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let k = tile(at: convert(event.locationInWindow, from: nil)), !disabled.contains(choices[k]) else { return }
        toggled(choices[k])
    }

    private func tile(at point: NSPoint) -> Int? {
        guard !choices.isEmpty, bounds.contains(point) else { return nil }
        let k = Int((point.x - Self.inset) / tileWidth)
        return k >= 0 && k < choices.count ? k : nil
    }

    private var tileWidth: CGFloat { (bounds.width - 2 * Self.inset) / CGFloat(max(choices.count, 1)) }

    private func frame(of k: Int) -> NSRect {
        NSRect(x: Self.inset + tileWidth * CGFloat(k), y: 2, width: tileWidth, height: bounds.height - 4)
    }

    override func draw(_ dirtyRect: NSRect) {
        let font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        for (k, surface) in choices.enumerated() {
            let tile = frame(of: k)
            if hovered == k {
                NSColor.quaternaryLabelColor.setFill()
                NSBezierPath(roundedRect: tile.insetBy(dx: 2, dy: 0), xRadius: MenuToggle.highlightRadius, yRadius: MenuToggle.highlightRadius).fill()
            }
            let lit = on.contains(surface)
            let usable = !disabled.contains(surface)
            let color: NSColor = !usable ? .quaternaryLabelColor : lit ? .labelColor : .tertiaryLabelColor
            let box = NSRect(x: tile.midX - Self.icon / 2, y: tile.minY + 6, width: Self.icon, height: Self.icon)
            Self.drawIcon(surface, in: box, color: color)
            let ink: NSColor = !usable ? .quaternaryLabelColor : lit ? .labelColor : .secondaryLabelColor
            let name = NSAttributedString(string: Self.name(of: surface), attributes: [.font: font, .foregroundColor: ink])
            let size = name.size()
            name.draw(at: NSPoint(x: tile.midX - size.width / 2, y: tile.maxY - size.height - 4))
        }
    }

    static func name(of surface: Surfaces) -> String {
        switch surface {
        case .ground: tr("surface.ground")
        case .dock: tr("surface.dock")
        case .widgets: tr("surface.widgets")
        case .selection: tr("surface.selection")
        default: tr("surface.windows")
        }
    }

    static func drawIcon(_ surface: Surfaces, in box: NSRect, color: NSColor, pointSize: CGFloat = 18) {
        guard surface == .ground else {
            let symbol = surface == .dock ? "dock.rectangle" : surface == .widgets ? "widget.small" : surface == .selection ? "rectangle.dashed" : "macwindow"
            guard let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
                .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: pointSize, weight: .medium).applying(.init(paletteColors: [color])))
            else { return }
            let size = image.size
            image.draw(in: NSRect(x: box.midX - size.width / 2, y: box.midY - size.height / 2, width: size.width, height: size.height))
            return
        }
        // A drift on the ground: no symbol shows one. Drawn for a 20 pt box and scaled to this one. A symbol drawn in a
        // see-through colour comes out with that colour's alpha applied twice, so the drift takes it twice too to match.
        let k = box.width / 20
        let alpha = color.usingColorSpace(.deviceRGB)?.alphaComponent ?? 1
        let ink = color.withAlphaComponent(alpha * alpha)
        let base = box.maxY - 3 * k
        let mound = NSBezierPath()
        mound.move(to: NSPoint(x: box.minX + 2 * k, y: base))
        mound.curve(to: NSPoint(x: box.maxX - 2 * k, y: base), controlPoint1: NSPoint(x: box.minX + 4 * k, y: base - 9 * k), controlPoint2: NSPoint(x: box.maxX - 4 * k, y: base - 9 * k))
        ink.withAlphaComponent(alpha * alpha * 0.3).setFill()
        mound.fill()
        ink.setStroke()
        mound.lineWidth = 1.7 * k
        mound.stroke()
        let line = NSBezierPath()
        line.move(to: NSPoint(x: box.minX, y: base))
        line.line(to: NSPoint(x: box.maxX, y: base))
        line.lineWidth = 1.7 * k
        line.lineCapStyle = .round
        line.stroke()
    }

    /// One icon to accessibility: a checkbox named for its surface.
    private final class Tile: NSAccessibilityElement {
        let surface: Surfaces

        init(_ surface: Surfaces) {
            self.surface = surface
            super.init()
            setAccessibilityRole(.checkBox)
            setAccessibilityLabel(SurfacePicker.name(of: surface))
        }

        private var picker: SurfacePicker? { accessibilityParent() as? SurfacePicker }

        override func accessibilityValue() -> Any? { picker?.on.contains(surface) == true ? 1 : 0 }

        override func isAccessibilityEnabled() -> Bool { picker?.disabled.contains(surface) != true }

        override func accessibilityFrame() -> NSRect {
            guard let picker, let k = picker.choices.firstIndex(of: surface), let window = picker.window else { return .zero }
            return window.convertToScreen(picker.convert(picker.frame(of: k), to: nil))
        }

        override func accessibilityPerformPress() -> Bool {
            picker?.toggled(surface)
            return true
        }
    }
}
