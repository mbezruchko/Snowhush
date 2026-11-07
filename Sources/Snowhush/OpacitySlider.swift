import AppKit

/// The Opacity menu control: how opaque the falling snow, leaves and rain are, with the percentage under it.
final class OpacitySlider: NSView {
    static let range = 0.1...1.0

    var changed: ((Double) -> Void)?

    var opacity = 1.0 {
        didSet { show() }
    }
    private var sent = 1.0

    private let slider = NSSlider()
    private let caption = NSTextField(labelWithString: "")

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 50))
        autoresizingMask = .width

        slider.minValue = Self.range.lowerBound
        slider.maxValue = Self.range.upperBound
        slider.isContinuous = true
        slider.target = self
        slider.action = #selector(slide)

        caption.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        caption.textColor = .secondaryLabelColor
        caption.alignment = .center

        for view in [slider, caption] {
            view.translatesAutoresizingMaskIntoConstraints = false
            addSubview(view)
        }
        // The track's ends line up with the item titles and the separators of a macOS 26 menu, as the Intensity slider's.
        NSLayoutConstraint.activate([
            slider.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 29),
            slider.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -15),
            slider.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            caption.centerXAnchor.constraint(equalTo: slider.centerXAnchor),
            caption.topAnchor.constraint(equalTo: slider.bottomAnchor, constant: 2),
            caption.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -2),
        ])
        setFrameSize(NSSize(width: frame.width, height: fittingSize.height))
        show()
    }

    required init?(coder: NSCoder) { nil }

    @objc private func slide() {
        let next = (slider.doubleValue * 20).rounded() / 20
        describe(next)
        guard next != sent else { return }
        sent = next
        changed?(next)
    }

    private func show() {
        slider.doubleValue = opacity
        sent = opacity
        describe(opacity)
    }

    private func describe(_ value: Double) {
        let text = "\(Int((value * 100).rounded()))%"
        slider.setAccessibilityValueDescription(text)
        caption.stringValue = text
    }
}

