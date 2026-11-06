import AppKit

/// The Rain Sounds menu control: a volume slider with the level written under it.
final class RainVolumeSlider: NSView {
    var changed: ((Float) -> Void)?

    var volume: Float = 0.5 {
        didSet { show() }
    }

    var enabled = true {
        didSet { slider.isEnabled = enabled }
    }

    private let slider = NSSlider()
    private let caption = NSTextField(labelWithString: "")

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 200, height: 50))
        autoresizingMask = .width

        slider.minValue = 0
        slider.maxValue = 1
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
        // The track's ends line up with the item titles and the separators of a macOS 26 menu.
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
        let next = Float(slider.doubleValue)
        guard abs(next - volume) > 0.001 else { return }
        volume = next
        changed?(next)
    }

    private func show() {
        slider.doubleValue = Double(volume)
        slider.isEnabled = enabled
        caption.stringValue = volume <= 0.001 ? "Muted" : "Volume \(Int((volume * 100).rounded()))%"
        slider.setAccessibilityValueDescription(caption.stringValue)
    }
}
