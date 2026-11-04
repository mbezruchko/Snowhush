import AppKit

enum Blossom {
    /// The menu bar icon while cherry blossom petals fall: a flower of five petals, each notched at its tip, drawn in
    /// outline like the symbols for the other modes.
    static let icon: NSImage = {
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: false) { _ in
            NSColor.black.setStroke()
            for k in 0..<5 {
                let petal = NSBezierPath()
                petal.move(to: NSPoint(x: 0, y: 1.9))
                petal.curve(to: NSPoint(x: 1.3, y: 7.4), controlPoint1: NSPoint(x: 3.0, y: 3.6), controlPoint2: NSPoint(x: 3.3, y: 6.9))
                petal.line(to: NSPoint(x: 0, y: 6.4))
                petal.line(to: NSPoint(x: -1.3, y: 7.4))
                petal.curve(to: NSPoint(x: 0, y: 1.9), controlPoint1: NSPoint(x: -3.3, y: 6.9), controlPoint2: NSPoint(x: -3.0, y: 3.6))
                petal.close()
                var transform = AffineTransform(translationByX: 9, byY: 9)
                transform.rotate(byDegrees: CGFloat(k) * 72)
                petal.transform(using: transform)
                petal.lineWidth = 1.3
                petal.lineJoinStyle = .round
                petal.stroke()
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Snowhush"
        return image
    }()
}
