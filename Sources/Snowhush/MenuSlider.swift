import AppKit

/// A slider in the menu, in steps of 5%, with the percentage under it: Intensity, Opacity. The view follows the
/// pointer itself: the slider's own tracking runs a nested event loop, which falls out of step with the menu's and
/// leaves the knob stuck now and then.
final class MenuSlider: NSView {
    var changed: ((Double) -> Void)?

    var value = 1.0 {
        didSet { show() }
    }
    private var sent = 1.0

    private let slider = NSSlider()
    private let caption = NSTextField(labelWithString: "")

    /// `captioned` false leaves out the percentage under the track, for a slider whose row above already shows it.
    init(range: ClosedRange<Double>, captioned: Bool = true) {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 50))
        autoresizingMask = .width

        slider.minValue = range.lowerBound
        slider.maxValue = range.upperBound
        slider.isContinuous = true
        slider.target = self
        slider.action = #selector(slide)

        caption.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        caption.textColor = .secondaryLabelColor
        caption.alignment = .center

        for view in captioned ? [slider, caption] : [slider] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        // The track starts under the section header text of a macOS 26 menu.
        NSLayoutConstraint.activate([
            slider.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            slider.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            slider.topAnchor.constraint(equalTo: topAnchor, constant: 4),
        ] + (captioned ? [
            caption.centerXAnchor.constraint(equalTo: slider.centerXAnchor),
            caption.topAnchor.constraint(equalTo: slider.bottomAnchor, constant: 2),
            caption.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -2),
        ] : [
            slider.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
        ]))
        setFrameSize(NSSize(width: frame.width, height: fittingSize.height))
        show()
    }

    required init?(coder: NSCoder) { nil }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    /// Where the knob was last pressed or dragged to, while it is: the slider's cell is told about the drag so its
    /// knob turns to glass as under the system's own tracking, which the view does without.
    private var pressed: NSPoint?

    override func mouseDown(with event: NSEvent) {
        let point = slider.convert(event.locationInWindow, from: nil)
        _ = slider.cell?.startTracking(at: point, in: slider)
        pressed = point
        follow(event)
    }

    override func mouseDragged(with event: NSEvent) {
        let point = slider.convert(event.locationInWindow, from: nil)
        if let last = pressed { _ = slider.cell?.continueTracking(last: last, current: point, in: slider) }
        pressed = point
        follow(event)
    }

    override func mouseUp(with event: NSEvent) { release() }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        release()
    }

    private func release() {
        guard let last = pressed else { return }
        pressed = nil
        slider.cell?.stopTracking(last: last, current: last, in: slider, mouseIsUp: true)
        slider.doubleValue = sent
    }

    /// Moves the knob to the pointer, to the nearest step.
    private func follow(_ event: NSEvent) {
        let x = slider.convert(event.locationInWindow, from: nil).x
        let knob = (slider.cell as? NSSliderCell)?.knobThickness ?? 0
        let width = slider.bounds.width - knob
        let along = width > 0 ? min(max((x - knob / 2) / width, 0), 1) : 0
        set((slider.minValue + along * (slider.maxValue - slider.minValue)) * 20)
    }

    /// Moved by accessibility.
    @objc private func slide() {
        set(slider.doubleValue * 20)
    }

    private func set(_ steps: Double) {
        let next = steps.rounded() / 20
        slider.doubleValue = next
        describe(next)
        guard next != sent else { return }
        sent = next
        changed?(next)
    }

    private func show() {
        slider.doubleValue = value
        sent = value
        describe(value)
    }

    static func percent(_ value: Double) -> String {
        value == 0 ? tr("off") : "\(Int((value * 100).rounded()))%"
    }

    private func describe(_ value: Double) {
        let text = Self.percent(value)
        slider.setAccessibilityValueDescription(text)
        caption.stringValue = text
    }
}

