import AppKit

/// Halloween: from October 30, when the update bringing it comes, until dawn on November 1 the snow takes on a
/// spooky mood.
enum Halloween {
    /// Whether it is Halloween, wanted or not.
    /// Lets it be previewed on any day: defaults write com.mbezruchko.Snowhush previewHalloween -bool true
    static func isSeason(at date: Date = Date()) -> Bool {
        if UserDefaults.standard.bool(forKey: "previewHalloween") { return true }
        let parts = Calendar.current.dateComponents([.month, .day, .hour], from: date)
        return parts.month == 10 && (parts.day ?? 0) >= 30 || parts.month == 11 && parts.day == 1 && (parts.hour ?? 0) < 6
    }

    static var isOn: Bool { isSeason() && Preferences.halloween }

    /// A jack-o'-lantern in place of the snowflake in the menu bar.
    static let icon: NSImage = {
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: false) { _ in
            NSColor.black.setFill()
            let body = NSBezierPath()
            body.appendOval(in: NSRect(x: 1.5, y: 1.5, width: 9, height: 11.5))
            body.appendOval(in: NSRect(x: 7.5, y: 1.5, width: 9, height: 11.5))
            body.appendOval(in: NSRect(x: 4.5, y: 1, width: 9, height: 12.5))
            body.fill()
            let stem = NSBezierPath()
            stem.move(to: NSPoint(x: 8.2, y: 12.6))
            stem.curve(to: NSPoint(x: 10.6, y: 16.6), controlPoint1: NSPoint(x: 8.4, y: 14.8), controlPoint2: NSPoint(x: 9.2, y: 16.2))
            stem.line(to: NSPoint(x: 11.4, y: 15.6))
            stem.curve(to: NSPoint(x: 10, y: 12.6), controlPoint1: NSPoint(x: 10.4, y: 15), controlPoint2: NSPoint(x: 10, y: 14))
            stem.close()
            stem.fill()

            NSGraphicsContext.current?.compositingOperation = .destinationOut
            let face = NSBezierPath()
            for x: CGFloat in [4.9, 10.3] {
                face.move(to: NSPoint(x: x, y: 8))
                face.line(to: NSPoint(x: x + 2.8, y: 8))
                face.line(to: NSPoint(x: x + 1.4, y: 10.3))
                face.close()
            }
            let mouth: [(CGFloat, CGFloat)] = [
                (4.4, 6.2), (6.2, 5.4), (7.3, 6.2), (9, 5.4), (10.7, 6.2), (11.8, 5.4), (13.6, 6.2),
                (12.2, 4), (9, 3.2), (5.8, 4),
            ]
            face.move(to: NSPoint(x: mouth[0].0, y: mouth[0].1))
            for point in mouth.dropFirst() { face.line(to: NSPoint(x: point.0, y: point.1)) }
            face.close()
            face.fill()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Snowhush"
        return image
    }()
}
