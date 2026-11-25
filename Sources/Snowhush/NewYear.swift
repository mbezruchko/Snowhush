import AppKit

/// The New Year: a decorated fir stands in the snow with presents under it from December 20 until dawn on January 8,
/// and any other day it is turned on from the menu.
enum NewYear {
    /// Whether it is the season, wanted or not.
    /// Lets it be previewed on any day: defaults write com.mbezruchko.Snowhush previewNewYear -bool true
    static func isSeason(at date: Date = Date()) -> Bool {
        if UserDefaults.standard.bool(forKey: "previewNewYear") { return true }
        let parts = Calendar.current.dateComponents([.month, .day, .hour], from: date)
        return parts.month == 12 && (parts.day ?? 0) >= 20 || parts.month == 1 && ((parts.day ?? 0) < 8 || parts.day == 8 && (parts.hour ?? 0) < 6)
    }

    /// Seconds past the New Year's midnight, from ten seconds before it, for the countdown, until the fireworks are
    /// over; nil at any other time.
    /// Lets it be previewed, 5 seconds after launch and every two minutes after:
    /// defaults write com.mbezruchko.Snowhush previewMidnight -bool true
    static func celebration(at date: Date = Date()) -> Double? {
        let t: Double
        if previewsMidnight {
            t = date.timeIntervalSince(launched).truncatingRemainder(dividingBy: 120) - 15
        } else {
            let parts = Calendar.current.dateComponents([.year, .month], from: date)
            guard parts.month == 12 || parts.month == 1,
                  let midnight = Calendar.current.date(from: DateComponents(year: (parts.year ?? 0) + (parts.month == 12 ? 1 : 0), month: 1, day: 1))
            else { return nil }
            t = date.timeIntervalSince(midnight)
        }
        return t >= -10 && t < 75 ? t : nil
    }

    /// How many rockets go up a minute through the season: three, and more and more the nearer the New Year is,
    /// twenty at midnight, falling back to three over the hour after.
    static func fireworksPerMinute() -> Float {
        let now = Date()
        if let rate = fireworksRate, now.timeIntervalSince(rate.at) < 2 { return rate.value }
        let value = fireworksPerMinute(at: now)
        fireworksRate = (value, now)
        return value
    }

    /// Worked out at most every two seconds: it goes by the calendar, which is slow to ask every frame.
    private static var fireworksRate: (value: Float, at: Date)?

    static func fireworksPerMinute(at date: Date) -> Float {
        guard isSeason(at: date) else { return 0 }
        // By the calendar even while midnight is previewed: that shows the countdown and its first rocket only.
        let parts = Calendar.current.dateComponents([.year, .month], from: date)
        guard let midnight = Calendar.current.date(from: DateComponents(year: (parts.year ?? 0) + (parts.month == 12 ? 1 : 0), month: 1, day: 1))
        else { return 3 }
        let t = date.timeIntervalSince(midnight)
        let rise = t < 0 ? exp(t / (3 * 24 * 3600)) : exp(-t / 1800)
        return Float(3 + 17 * rise)
    }

    private static let previewsMidnight = UserDefaults.standard.bool(forKey: "previewMidnight")
    private static let launched = Date()

    /// Whether the New Year is wanted today: turned on, or come with its season and not turned off for it.
    static var isWanted: Bool {
        Preferences.newYear || isSeason() && Preferences.newYearSkipped != season
    }

    /// Whether the fir stands in the snow: wanted, snow falling and Halloween not keeping its own season.
    static var isOn: Bool { Holiday.current == .newYear }

    /// Turned on in season, it ends with the season; any other day, it stays on. Turning it on puts Halloween away.
    static func toggle() {
        if isWanted {
            turnOff()
        } else {
            Halloween.turnOff()
            if isSeason() { Preferences.newYearSkipped = 0 } else { Preferences.newYear = true }
        }
    }

    static func turnOff() {
        Preferences.newYear = false
        if isSeason() { Preferences.newYearSkipped = season }
    }

    /// The year the season began in: its days in January belong to the December before.
    private static var season: Int {
        let parts = Calendar.current.dateComponents([.year, .month], from: Date())
        return (parts.year ?? 0) - (parts.month == 1 ? 1 : 0)
    }

    /// A fir with a star in place of the snowflake in the menu bar.
    static let icon: NSImage = {
        let image = NSImage(size: NSSize(width: 18, height: 18), flipped: false) { _ in
            NSColor.black.setFill()
            let tiers: [(bottom: CGFloat, half: CGFloat, top: CGFloat)] = [(3, 7, 9), (6.5, 5.6, 11.8), (9.6, 4.2, 14.2)]
            for tier in tiers {
                let path = NSBezierPath()
                path.move(to: NSPoint(x: 9 - tier.half, y: tier.bottom))
                path.line(to: NSPoint(x: 9 + tier.half, y: tier.bottom))
                path.line(to: NSPoint(x: 9, y: tier.top))
                path.close()
                path.fill()
            }
            NSBezierPath(rect: NSRect(x: 8, y: 1, width: 2, height: 2.5)).fill()
            let star = NSBezierPath()
            for k in 0..<10 {
                let angle = CGFloat(k) * .pi / 5 + .pi / 2
                let radius: CGFloat = k % 2 == 0 ? 2.4 : 1
                let point = NSPoint(x: 9 + cos(angle) * radius, y: 15.4 + sin(angle) * radius)
                if k == 0 { star.move(to: point) } else { star.line(to: point) }
            }
            star.close()
            star.fill()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Snowhush"
        return image
    }()
}

/// The holiday the snow keeps, if any.
enum Holiday {
    case halloween, newYear

    /// One in its season comes first, then one turned on for every day; only while snow falls on its own.
    static var current: Holiday? {
        guard Preferences.modes == [.snow] else { return nil }
        if NewYear.isSeason(), NewYear.isWanted { return .newYear }
        if Halloween.isSeason(), Halloween.isWanted { return .halloween }
        if Preferences.newYear { return .newYear }
        if Preferences.halloween { return .halloween }
        return nil
    }
}
