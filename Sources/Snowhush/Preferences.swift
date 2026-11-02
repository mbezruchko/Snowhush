import AppKit

enum Snowfall: Int, CaseIterable {
    case off, light, steady, heavy, variable

    var title: String {
        switch self {
        case .off: "Off"
        case .light: "Light"
        case .steady: "Steady"
        case .heavy: "Heavy"
        case .variable: "Variable"
        }
    }

    var flakesPerMegapoint: Float {
        switch self {
        case .off: 0
        case .light: 150
        case .steady: 380
        case .heavy: 800
        case .variable: 380
        }
    }
}

enum Preferences {
    private static let defaults = UserDefaults.standard
    private static let snowfallKey = "snowfall"
    private static let behindWindowsKey = "behindWindows"
    private static let showOverFullScreenKey = "showOverFullScreen"
    private static let meltSnowKey = "meltSnow"
    private static let accumulateKey = "accumulate"
    private static let snowOnWindowsKey = "snowOnWindows"
    private static let welcomedKey = "welcomed"
    private static let showInScreenshotsKey = "showInScreenshots"
    private static let halloweenKey = "halloweenOn"
    private static let halloweenSkippedKey = "halloweenSkipped"
    private static let autumnKey = "autumn"

    static func registerDefaults() {
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        defaults.register(defaults: [
            snowfallKey: (calm ? Snowfall.light : .steady).rawValue,
            behindWindowsKey: false,
            showOverFullScreenKey: false,
            meltSnowKey: true,
            accumulateKey: true,
            snowOnWindowsKey: true,
            showInScreenshotsKey: false,
            halloweenKey: false,
            halloweenSkippedKey: 0,
            autumnKey: false,
        ])
    }

    static var snowfall: Snowfall {
        get { Snowfall(rawValue: defaults.integer(forKey: snowfallKey)) ?? .steady }
        set { defaults.set(newValue.rawValue, forKey: snowfallKey) }
    }

    static var snowOnWindows: Bool {
        get { defaults.bool(forKey: snowOnWindowsKey) }
        set { defaults.set(newValue, forKey: snowOnWindowsKey) }
    }

    static var accumulate: Bool {
        get { defaults.bool(forKey: accumulateKey) }
        set { defaults.set(newValue, forKey: accumulateKey) }
    }

    static var meltSnow: Bool {
        get { defaults.bool(forKey: meltSnowKey) }
        set { defaults.set(newValue, forKey: meltSnowKey) }
    }

    static var welcomed: Bool {
        get { defaults.bool(forKey: welcomedKey) }
        set { defaults.set(newValue, forKey: welcomedKey) }
    }

    static var behindWindows: Bool {
        get { defaults.bool(forKey: behindWindowsKey) }
        set { defaults.set(newValue, forKey: behindWindowsKey) }
    }

    static var showOverFullScreen: Bool {
        get { defaults.bool(forKey: showOverFullScreenKey) }
        set { defaults.set(newValue, forKey: showOverFullScreenKey) }
    }

    static var showInScreenshots: Bool {
        get { defaults.bool(forKey: showInScreenshotsKey) }
        set { defaults.set(newValue, forKey: showInScreenshotsKey) }
    }

    /// Halloween turned on from the menu, for every day.
    static var halloween: Bool {
        get { defaults.bool(forKey: halloweenKey) }
        set { defaults.set(newValue, forKey: halloweenKey) }
    }

    /// The year whose Halloween was turned off from the menu, zero for none.
    static var halloweenSkipped: Int {
        get { defaults.integer(forKey: halloweenSkippedKey) }
        set { defaults.set(newValue, forKey: halloweenSkippedKey) }
    }

    static var autumn: Bool {
        get { defaults.bool(forKey: autumnKey) }
        set { defaults.set(newValue, forKey: autumnKey) }
    }
}
