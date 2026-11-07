import AppKit

enum Preferences {
    private static let defaults = UserDefaults.standard
    static let opacityRange = 0.1...1.0
    private static let intensityKey = "intensity"
    private static let varyIntensityKey = "variableIntensity"
    /// Before the slider, the intensity was one of Off, Light, Steady, Heavy and Variable.
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
    private static let fallingOpacityKey = "fallingOpacity"

    static func registerDefaults() {
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        defaults.register(defaults: [
            intensityKey: calm ? 0.25 : 0.5,
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
            fallingOpacityKey: 1.0,
        ])
        if defaults.object(forKey: intensityKey) == nil, let stop = defaults.object(forKey: snowfallKey) as? Int {
            defaults.set([0, 0.25, 0.5, 0.75, 0.5][min(max(stop, 0), 4)], forKey: intensityKey)
            defaults.set(stop == 4, forKey: varyIntensityKey)
        }
    }

    /// How much falls, from 0 to 1.
    static var intensity: Double {
        get { min(max(defaults.double(forKey: intensityKey), 0), 1) }
        set { defaults.set(newValue, forKey: intensityKey) }
    }

    /// Whether the snowfall rises and eases on its own over the day.
    static var variesIntensity: Bool {
        get { defaults.bool(forKey: varyIntensityKey) }
        set { defaults.set(newValue, forKey: varyIntensityKey) }
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

    /// How opaque snow, leaves and rain are while they fall.
    static var fallingOpacity: Double {
        get { min(max(defaults.double(forKey: fallingOpacityKey), opacityRange.lowerBound), opacityRange.upperBound) }
        set { defaults.set(newValue, forKey: fallingOpacityKey) }
    }

    /// Drawn at the display's highest rate all the time, instead of only while something is pushed around.
    static var maxPerformance: Bool {
        get { defaults.integer(forKey: performanceKey) == 1 }
        set { defaults.set(newValue ? 1 : 0, forKey: performanceKey) }
    }
}
