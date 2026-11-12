import AppKit

/// What snow and leaves settle on.
struct Surfaces: OptionSet {
    let rawValue: Int
    static let ground = Surfaces(rawValue: 1)
    static let dock = Surfaces(rawValue: 2)
    static let windows = Surfaces(rawValue: 4)
    static let widgets = Surfaces(rawValue: 8)
    /// What the window list is read for.
    static let tracked: Surfaces = [.dock, .windows, .widgets]
}

enum Preferences {
    private static let defaults = UserDefaults.standard
    static var weatherEnabled: Bool {
        get { defaults.bool(forKey: "weatherEnabled") }
        set { defaults.set(newValue, forKey: "weatherEnabled") }
    }
    static var weatherAutomatic: Bool {
        get { defaults.object(forKey: "weatherAutomatic") as? Bool ?? true }
        set { defaults.set(newValue, forKey: "weatherAutomatic") }
    }
    static var weatherEffects: WeatherEffects {
        get {
            guard let data = defaults.data(forKey: "weatherEffects"),
                  let effects = try? JSONDecoder().decode(WeatherEffects.self, from: data) else { return .allWeather }
            return effects
        }
        set { defaults.set(try? JSONEncoder().encode(newValue), forKey: "weatherEffects") }
    }
    static var weatherPlace: WeatherPlace? {
        get {
            guard let data = defaults.data(forKey: "weatherPlace"),
                  let place = try? JSONDecoder().decode(WeatherPlace.self, from: data), place.isValid else { return nil }
            return place
        }
        set { defaults.set(newValue.flatMap { try? JSONEncoder().encode($0) }, forKey: "weatherPlace") }
    }
    static let opacityRange = 0.1...1.0
    private static let intensityKey = "intensity"
    /// Before the slider, the intensity was one of Off, Light, Steady, Heavy and Variable.
    private static let snowfallKey = "snowfall"
    private static let behindWindowsKey = "behindWindows"
    private static let showOverFullScreenKey = "showOverFullScreen"
    private static let meltSnowKey = "meltSnow"
    /// Before Accumulate picked where snow settles, it turned settling off everywhere at once.
    private static let accumulateKey = "accumulate"
    private static let groundKey = "settleGround"
    private static let dockKey = "settleDock"
    private static let widgetsKey = "settleWidgets"
    private static let snowOnWindowsKey = "snowOnWindows"
    private static let welcomedKey = "welcomed"
    private static let installedAtKey = "installedAt"
    private static let showInScreenshotsKey = "showInScreenshots"
    private static let deepInteractionKey = "deepInteraction"
    private static let halloweenKey = "halloweenOn"
    private static let halloweenSkippedKey = "halloweenSkipped"
    private static let modeKey = "mode"
    private static let mixedKey = "mixed"
    private static let mixKey = "mix"
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
            groundKey: true,
            dockKey: true,
            widgetsKey: true,
            snowOnWindowsKey: true,
            showInScreenshotsKey: false,
            deepInteractionKey: false,
            halloweenKey: false,
            halloweenSkippedKey: 0,
            autumnKey: false,
            mixedKey: false,
            fallingOpacityKey: 1.0,
        ])
        if defaults.object(forKey: intensityKey) == nil, let stop = defaults.object(forKey: snowfallKey) as? Int {
            defaults.set([0, 0.25, 0.5, 0.75, 0.5][min(max(stop, 0), 4)], forKey: intensityKey)
        }
        if defaults.object(forKey: accumulateKey) != nil {
            if !defaults.bool(forKey: accumulateKey) {
                for key in [groundKey, dockKey, widgetsKey, snowOnWindowsKey] { defaults.set(false, forKey: key) }
            }
            defaults.removeObject(forKey: accumulateKey)
        }
        if defaults.object(forKey: installedAtKey) == nil {
            defaults.set(defaults.bool(forKey: welcomedKey) ? 0 : Date().timeIntervalSince1970, forKey: installedAtKey)
        }
    }

    /// When this install was first run, or nil for an install older than the stamp.
    static var installedAt: Date? {
        let stamp = defaults.double(forKey: installedAtKey)
        return stamp > 0 ? Date(timeIntervalSince1970: stamp) : nil
    }

    /// How much falls, from 0 to 1.
    static var intensity: Double {
        get { min(max(defaults.double(forKey: intensityKey), 0), 1) }
        set { defaults.set(newValue, forKey: intensityKey) }
    }

    /// Where snow settles; Windows is `snowOnWindows`.
    static var surfaces: Surfaces {
        get {
            var surfaces: Surfaces = []
            for (surface, key) in [(Surfaces.ground, groundKey), (.dock, dockKey), (.widgets, widgetsKey), (.windows, snowOnWindowsKey)]
            where defaults.bool(forKey: key) {
                surfaces.insert(surface)
            }
            return surfaces
        }
        set {
            for (surface, key) in [(Surfaces.ground, groundKey), (.dock, dockKey), (.widgets, widgetsKey), (.windows, snowOnWindowsKey)] {
                defaults.set(newValue.contains(surface), forKey: key)
            }
        }
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

    /// Two modes falling together.
    static var mixed: Bool {
        get { defaults.bool(forKey: mixedKey) }
        set { defaults.set(newValue, forKey: mixedKey) }
    }

    /// The two modes Mixed lets fall together, the one picked last at the end.
    static var mix: [Mode] {
        get {
            let picked = (defaults.array(forKey: mixKey) as? [Int] ?? []).compactMap(Mode.init(rawValue:))
            return picked.count == 2 && picked[0] != picked[1] ? picked : [mode == .snow ? .autumn : .snow, mode]
        }
        set { defaults.set(newValue.map(\.rawValue), forKey: mixKey) }
    }

    /// What falls.
    static var modes: [Mode] { mixed ? mix : [mode] }

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
