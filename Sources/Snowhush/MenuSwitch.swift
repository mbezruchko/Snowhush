import AppKit

/// A menu row with a switch at its right end, as the system's own menu bar menus have them: a click anywhere on the
/// row flips it, and the menu stays open. It shows its enclosing item's title.
final class MenuSwitch: NSView {
    var toggled: (Bool) -> Void = { _ in }

    var isOn: Bool {
        get { toggle.state == .on }
        set { toggle.state = newValue ? .on : .off }
    }

    var isEnabled = true {
        didSet {
            toggle.isEnabled = isEnabled
            needsDisplay = true
        }
    }

    private let toggle = NSSwitch()

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: MenuToggle.rowHeight + 4))
        autoresizingMask = .width
        toggle.controlSize = .mini
        toggle.target = self
        toggle.action = #selector(flipped)
        toggle.translatesAutoresizingMaskIntoConstraints = false
        addSubview(toggle)
        NSLayoutConstraint.activate([
            toggle.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -14),
            toggle.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    /// The switch would track the pointer in a loop of its own, out of step with the menu's, as a slider does.
    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    override func mouseUp(with event: NSEvent) {
        guard isEnabled, bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        toggle.performClick(nil)
    }

    @objc private func flipped() {
        toggled(isOn)
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        toggle.setAccessibilityLabel(enclosingMenuItem?.title)
    }

    /// Widens the row to fit `title` and the switch: a menu sizes itself to its rows' views.
    func fit(_ title: String) {
        let text = NSAttributedString(string: title, attributes: [.font: NSFont.menuFont(ofSize: 0)])
        toggle.setAccessibilityLabel(title)
        setFrameSize(NSSize(width: max(200, ceil(MenuDisclosure.titleX + text.size().width + 24 + toggle.fittingSize.width + 14)), height: frame.height))
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let item = enclosingMenuItem else { return }
        let color: NSColor = isEnabled ? .labelColor : .disabledControlTextColor
        let title = NSAttributedString(string: item.title, attributes: [.font: NSFont.menuFont(ofSize: 0), .foregroundColor: color])
        title.draw(at: NSPoint(x: MenuDisclosure.titleX, y: (bounds.height - title.size().height) / 2))
    }
}
