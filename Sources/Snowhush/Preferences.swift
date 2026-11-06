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
    private static let deepInteractionKey = "deepInteraction"
    private static let halloweenKey = "halloweenOn"
    private static let halloweenSkippedKey = "halloweenSkipped"
    private static let modeKey = "mode"
    /// Before rain, the mode was only whether leaves fell.
    private static let autumnKey = "autumn"
    private static let performanceKey = "performance"
    private static let rainSoundsKey = "rainSounds"
    private static let thunderKey = "thunder"
    private static let rainVolumeKey = "rainVolume"

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
            deepInteractionKey: false,
            halloweenKey: false,
            halloweenSkippedKey: 0,
            autumnKey: false,
            rainSoundsKey: true,
            thunderKey: true,
            rainVolumeKey: 0.5,
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

    /// The snow also settles on what the desktop draws without a window of its own: a selection dragged across it.
    static var deepInteraction: Bool {
        get { defaults.bool(forKey: deepInteractionKey) }
        set { defaults.set(newValue, forKey: deepInteractionKey) }
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

    static var mode: Mode {
        get {
            if let raw = defaults.object(forKey: modeKey) as? Int, let mode = Mode(rawValue: raw) { return mode }
            return defaults.bool(forKey: autumnKey) ? .autumn : .snow
        }
        set { defaults.set(newValue.rawValue, forKey: modeKey) }
    }

    /// Drawn at the display's highest rate all the time, instead of only while something is pushed around.
    static var maxPerformance: Bool {
        get { defaults.integer(forKey: performanceKey) == 1 }
        set { defaults.set(newValue ? 1 : 0, forKey: performanceKey) }
    }

    /// Play a rain bed and thunder while Rain is the chosen mode.
    static var rainSounds: Bool {
        get { defaults.bool(forKey: rainSoundsKey) }
        set { defaults.set(newValue, forKey: rainSoundsKey) }
    }

    /// Let thunder break now and then over the rain.
    static var thunder: Bool {
        get { defaults.bool(forKey: thunderKey) }
        set { defaults.set(newValue, forKey: thunderKey) }
    }

    /// How loud the rain and thunder are, from silent to full.
    static var rainVolume: Float {
        get { min(max(defaults.float(forKey: rainVolumeKey), 0), 1) }
        set { defaults.set(newValue, forKey: rainVolumeKey) }
    }
}
