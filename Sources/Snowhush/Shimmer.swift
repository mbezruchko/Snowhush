import AppKit

/// Text that shimmers through colours, as gradient names do on Discord: a gradient that runs round from pink through
/// violet and blue back to pink fills it, sliding along it.
enum Shimmer {
    private static let colors: [NSColor] = [
        NSColor(srgbRed: 1.0, green: 0.42, blue: 0.78, alpha: 1),
        NSColor(srgbRed: 0.78, green: 0.45, blue: 1.0, alpha: 1),
        NSColor(srgbRed: 0.45, green: 0.58, blue: 1.0, alpha: 1),
        NSColor(srgbRed: 0.36, green: 0.85, blue: 1.0, alpha: 1),
        NSColor(srgbRed: 0.78, green: 0.45, blue: 1.0, alpha: 1),
        NSColor(srgbRed: 1.0, green: 0.42, blue: 0.78, alpha: 1),
    ]
    /// How wide one round of the gradient is, in points: about the width of a menu item's title.
    private static let width: CGFloat = 140

    /// `text` in the menu's font, the gradient moved on by `phase`, 0 to 1 a round.
    static func title(_ text: String, phase: CGFloat) -> NSAttributedString {
        NSAttributedString(string: text, attributes: [.font: NSFont.menuFont(ofSize: 0), .foregroundColor: fill(phase)])
    }

    /// The gradient as a colour to fill text with: a picture of one round of it, tiled across, so that it is seamless.
    private static func fill(_ phase: CGFloat) -> NSColor {
        let offset = (phase - phase.rounded(.down)) * width
        let image = NSImage(size: NSSize(width: width, height: 32), flipped: false) { rect in
            guard let gradient = NSGradient(colors: colors) else { return false }
            gradient.draw(in: NSRect(x: -offset, y: 0, width: width, height: rect.height), angle: 0)
            gradient.draw(in: NSRect(x: width - offset, y: 0, width: width, height: rect.height), angle: 0)
            return true
        }
        return NSColor(patternImage: image)
    }
}
