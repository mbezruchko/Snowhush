import AppKit
import MetalKit
import ServiceManagement
import Sparkle

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate, SPUStandardUserDriverDelegate {
    private var device: MTLDevice!
    private var screens: [ScreenSnow] = []
    private let displays = Displays()
    private var surfaces: Surfaces = []
    private var thaws = true
    private var behindWindows = false
    private var snowfallHeld = false
    private var halloween = false
    private var statusItem: NSStatusItem!
    private let intensitySlider = MenuSlider(range: 0...1)
    private var intensityHeading: NSMenuItem!
    private let opacitySlider = MenuSlider(range: Preferences.opacityRange, captioned: false)
    private var opacityItem: NSMenuItem!
    private let opacityRow = MenuDisclosure()
    private var opacitySliderItem: NSMenuItem!
    private var accumulateItem: NSMenuItem!
    private let accumulateRow = MenuDisclosure()
    private var surfacesItem: NSMenuItem!
    private let surfacePicker = SurfacePicker()
    private var meltItem: NSMenuItem!
    private var clearItem: NSMenuItem!
    private var halloweenItem: NSMenuItem!
    private var mixedItem: NSMenuItem!
    private var modeItems: [(item: NSMenuItem, mode: Mode)] = []
    private let weather = WeatherSync()
    private var weatherWindow: WeatherWindow?
    private var weatherItem: NSMenuItem!
    private var weatherStatusItem: NSMenuItem!
    private var presentedModes: [Mode] = []
    private var wasWeatherEnabled = false
    private var wasPrecipitating = false
    private var maxPerformanceItem: NSMenuItem!
    private var behindWindowsItem: NSMenuItem!
    private var fullScreenItem: NSMenuItem!
    private var screenshotsItem: NSMenuItem!
    private var deepInteractionItem: NSMenuItem!
    private var loginItem: NSMenuItem!
    private var observers: [NSObjectProtocol] = []
    private var screensAsleep = false
    private let screenshots = ScreenshotWatch()
    private var screenLocked = false
    private var sessionInactive = false
    private var updater: SPUStandardUpdaterController!
    private var welcome: WelcomeController?
    private let power = PowerMonitor()
    private var halloweenTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if AppMover.moveIfNeeded() {
            NSApp.terminate(nil)
            return
        }
        Preferences.registerDefaults()
        guard let device = Self.lowPowerDevice() ?? MTLCreateSystemDefaultDevice() else {
            NSApp.terminate(nil)
            return
        }
        self.device = device
        surfaces = Preferences.surfaces
        thaws = Preferences.meltSnow
        behindWindows = Preferences.behindWindows
        snowfallHeld = !Preferences.welcomed
        syncScreens()
        guard !screens.isEmpty else {
            NSApp.terminate(nil)
            return
        }

        updater = SPUStandardUpdaterController(startingUpdater: true, updaterDelegate: nil, userDriverDelegate: self)
        statusItem = makeStatusItem()
        makeAppMenu()
        weather.changed = { [weak self] in self?.applyWeather() }
        presentedModes = Preferences.modes
        applyWeather()
        weather.start()
        updateStatusIcon()
        refreshHalloween()
        halloweenTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshHalloween() }
        }
        observeSystem()
        Marquee.enabled = Preferences.deepInteraction
        screenshots.stepAside = { [weak self] aside in self?.screens.forEach { $0.stepAside(aside) } }
        screenshots.start()
        if snowfallHeld {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in self?.showWelcome() }
        }
    }

    // A menu bar app has no Dock icon to badge, so update reminders are left to Sparkle's own gentle alerts.
    nonisolated var supportsGentleScheduledUpdateReminders: Bool { true }

    // MARK: - Displays

    /// One overlay per display: new displays get snow, removed ones lose theirs, the rest follow resolution changes.
    private func syncScreens() {
        let current = NSScreen.screens
        displays.update(screens: current)
        let ids = Set(current.compactMap(\.displayID))
        for gone in screens where !ids.contains(gone.displayID) { gone.close() }
        screens.removeAll { !ids.contains($0.displayID) }

        for screen in current {
            if let existing = screens.first(where: { $0.displayID == screen.displayID }) {
                existing.fit(to: screen)
            } else if let snow = ScreenSnow(screen: screen, device: device, displays: displays, surfaces: { [unowned self] in surfaces }) {
                snow.simulation.snowfallHeld = snowfallHeld
                snow.simulation.snowfallReleased = { [weak self] in self?.releaseSnowfall() }
                snow.halloween = halloween
                snow.modes = activeModes
                snow.simulation.intensity = activeIntensity
                snow.setWeather(weatherScene, strength: weatherIntensity.atmosphere)
                snow.maxFrameRate = Preferences.maxPerformance
                snow.activityChanged = { [weak self] in self?.updateActivity() }
                snow.apply(power.budget)
                screens.append(snow)
            }
        }
        displays.snows = screens
        updateActivity()
    }

    private func releaseSnowfall() {
        snowfallHeld = false
        for snow in screens { snow.simulation.snowfallHeld = false }
    }

    private func showWelcome() {
        let welcome = WelcomeController()
        welcome.scatter = { [weak self] pieces in
            guard let self, let first = pieces.first else { return }
            let point = CGPoint(x: CGFloat(first.position.x), y: CGFloat(first.position.y))
            guard let snow = screens.first(where: { $0.window.frame.contains(point) }) ?? screens.first else { return }
            let origin = SIMD2(Float(snow.window.frame.minX), Float(snow.window.frame.minY))
            snow.simulation.scatter(pieces.map { piece in
                var local = piece
                local.position -= origin
                return local
            })
        }
        welcome.skipped = { [weak self] in self?.releaseSnowfall() }
        welcome.finished = { [weak self] in
            Preferences.welcomed = true
            self?.welcome = nil
        }
        self.welcome = welcome
        welcome.show()
    }

    // MARK: - Menu

    /// Standard Settings shortcut while Snowhush is active, including its welcome and settings windows.
    private func makeAppMenu() {
        let main = NSMenu()
        let application = NSMenu()
        let settings = application.addItem(withTitle: tr("weather.configure"), action: #selector(showWeather), keyEquivalent: ",")
        settings.target = self
        application.addItem(.separator())
        application.addItem(withTitle: tr("quit"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let root = NSMenuItem()
        root.submenu = application
        main.addItem(root)
        NSApp.mainMenu = main
    }

    private func makeStatusItem() -> NSStatusItem {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)

        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false

        intensityHeading = .sectionHeader(title: tr("intensity"))
        menu.addItem(intensityHeading)
        intensitySlider.changed = { [unowned self] in select(intensity: $0) }
        let intensityItem = NSMenuItem()
        intensityItem.view = intensitySlider
        menu.addItem(intensityItem)
        // Opacity is set once and left, so it folds away like Accumulate, its value on the row.
        opacityItem = menu.addItem(withTitle: tr("opacity"), action: nil, keyEquivalent: "")
        opacityItem.view = opacityRow
        opacityRow.summaryWhileOpen = true
        opacityRow.toggled = { [unowned self] in
            opacityRow.open.toggle()
            opacitySliderItem.isHidden = !opacityRow.open
        }
        opacitySlider.changed = { [unowned self] opacity in
            Preferences.fallingOpacity = opacity
            for snow in screens { snow.simulation.fallingOpacity = Float(opacity) }
            opacityRow.summary = MenuSlider.percent(opacity)
        }
        opacitySliderItem = NSMenuItem()
        opacitySliderItem.view = opacitySlider
        opacitySliderItem.isHidden = true
        menu.addItem(opacitySliderItem)
        accumulateItem = menu.addItem(withTitle: tr("accumulate"), action: nil, keyEquivalent: "")
        accumulateItem.view = accumulateRow
        accumulateRow.toggled = { [unowned self] in
            accumulateRow.open.toggle()
            surfacesItem.isHidden = !accumulateRow.open
            if !accumulateRow.open { accumulateRow.fit(accumulateItem.title, least: surfacePicker.frame.width) }
        }
        surfacesItem = NSMenuItem()
        surfacesItem.view = surfacePicker
        surfacesItem.isHidden = true
        surfacePicker.toggled = { [unowned self] surface in toggle(surface) }
        menu.addItem(surfacesItem)
        let modes = NSMenu()
        modes.autoenablesItems = false
        weatherItem = toggle(tr("weather.effects"), #selector(toggleWeather), to: modes)
        (weatherItem.view as? MenuToggle)?.staysOpen = { false }
        add(tr("weather.configure"), #selector(showWeather), to: modes)
        weatherStatusItem = modes.addItem(withTitle: "", action: nil, keyEquivalent: "")
        weatherStatusItem.isEnabled = false
        modes.addItem(.separator())
        mixedItem = toggle(tr("mixed"), #selector(toggleMixed), to: modes)
        modes.addItem(.separator())
        modeItems = Mode.allCases.map { mode in
            let item = toggle(tr(mode.key), #selector(chooseMode), to: modes)
            (item.view as? MenuToggle)?.staysOpen = { Preferences.mixed }
            return (item, mode)
        }
        modes.addItem(.separator())
        halloweenItem = add(tr("halloween"), #selector(toggleHalloween), to: modes)
        menu.addItem(withTitle: tr("mode"), action: nil, keyEquivalent: "").submenu = modes
        clearItem = add(tr("clear.snow"), #selector(clearSnow), to: menu)
        menu.addItem(.separator())
        let settings = NSMenu()
        settings.autoenablesItems = false
        settings.addItem(.sectionHeader(title: tr("behavior")))
        meltItem = toggle(tr("melt.snow"), #selector(toggleMeltSnow), to: settings)
        deepInteractionItem = toggle(tr("deep"), #selector(toggleDeepInteraction), to: settings)
        settings.addItem(.separator())
        settings.addItem(.sectionHeader(title: tr("display")))
        behindWindowsItem = toggle(tr("behind"), #selector(toggleBehindWindows), to: settings)
        fullScreenItem = toggle(tr("full_screen"), #selector(toggleFullScreen), to: settings)
        screenshotsItem = toggle(tr("screenshots"), #selector(toggleScreenshots), to: settings)
        maxPerformanceItem = toggle(tr("performance"), #selector(toggleMaxPerformance), to: settings)
        settings.addItem(.separator())
        settings.addItem(.sectionHeader(title: tr("general")))
        loginItem = toggle(tr("login"), #selector(toggleOpenAtLogin), to: settings)
        let updates = settings.addItem(withTitle: tr("updates"), action: #selector(SPUStandardUpdaterController.checkForUpdates(_:)), keyEquivalent: "")
        updates.target = updater
        menu.addItem(withTitle: tr("settings"), action: nil, keyEquivalent: "").submenu = settings
        add(tr("support"), #selector(openSponsors), to: menu).attributedTitle = Self.supportTitle
        menu.addItem(withTitle: tr("quit"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")

        item.menu = menu
        return item
    }

    /// The site's sponsor heart, in its colour, ahead of the title: set as the item's image, the status menu doesn't
    /// show it.
    private static let supportTitle: NSAttributedString = {
        let font = NSFont.menuFont(ofSize: 0)
        let title = NSMutableAttributedString(string: tr("support"), attributes: [.font: font])
        guard let heart = NSImage(systemSymbolName: "heart.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: font.pointSize * 0.85, weight: .regular).applying(.init(paletteColors: [NSColor(srgbRed: 0.92, green: 0.51, blue: 0.51, alpha: 1)])))
        else { return title }
        heart.isTemplate = false
        let attachment = NSTextAttachment()
        attachment.image = heart
        attachment.bounds = CGRect(x: 0, y: (font.capHeight - heart.size.height) / 2, width: heart.size.width, height: heart.size.height)
        title.insert(NSAttributedString(string: "  ", attributes: [.font: font]), at: 0)
        title.insert(NSAttributedString(attachment: attachment), at: 0)
        return title
    }()

    @discardableResult
    private func add(_ title: String, _ action: Selector, to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: action, keyEquivalent: "")
        item.target = self
        return item
    }

    private func toggle(_ title: String, _ action: Selector, to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: nil, keyEquivalent: "")
        let toggle = MenuToggle(action: action, target: self)
        toggle.fit(title)
        item.view = toggle
        return item
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        let automaticIntensity = weather.enabled && Preferences.weatherAutomaticIntensity
        intensitySlider.value = automaticIntensity ? Double(activeIntensity) : Preferences.intensity
        intensityHeading.title = tr("intensity") + (automaticIntensity ? " · " + tr("weather.rule.weather") : "")
        intensitySlider.toolTip = automaticIntensity ? tr("weather.intensity.help") : nil
        opacitySlider.value = Preferences.fallingOpacity
        opacityRow.summary = MenuSlider.percent(Preferences.fallingOpacity)
        opacityRow.fit(opacityItem.title, least: 0)
        let modes = Mode.allCases.filter(activeModes.contains)
        weatherItem.state = weather.enabled ? .on : .off
        weatherStatusItem.isHidden = !weather.enabled
        weatherStatusItem.title = weatherSummary
        mixedItem.state = !weather.enabled && Preferences.mixed ? .on : .off
        for (item, mode) in modeItems { item.state = !weather.enabled && modes.contains(mode) ? .on : .off }
        // Rain neither heaps up nor lies about to clear away.
        let heaps = modes.filter { $0 != .rain }
        meltItem.isHidden = heaps.isEmpty
        clearItem.isHidden = heaps.isEmpty
        meltItem.title = tr(heaps.contains(.snow) ? "melt.snow" : heaps == [.blossom] ? "melt.blossom" : "melt.autumn")
        clearItem.title = tr("clear", heaps)
        (meltItem.view as? MenuToggle)?.fit(meltItem.title)
        // Top to bottom, as they lie on the screen. Rain splashes on the ground whatever it says.
        let choices: [Surfaces] = [.widgets, .windows, .dock] + (heaps.isEmpty ? [] : [.ground])
        if surfacePicker.choices != choices { surfacePicker.choices = choices }
        surfacePicker.on = surfaces
        let lit = choices.filter(surfaces.contains)
        let every = lit.count == choices.count
        accumulateRow.summary = every ? tr("all") : lit.isEmpty ? tr("off") : ""
        accumulateRow.summaryIcons = every || lit.isEmpty ? [] : lit
        // Unfolded, the summary isn't shown: the menu keeps its width while the icons are switched.
        if !accumulateRow.open { accumulateRow.fit(accumulateItem.title, least: surfacePicker.frame.width) }
        meltItem.state = thaws ? .on : .off
        meltItem.isEnabled = !surfaces.isEmpty
        deepInteractionItem.state = Preferences.deepInteraction ? .on : .off
        deepInteractionItem.isEnabled = surfaces.contains(.windows)
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
        halloweenItem.state = Halloween.isWanted ? .on : .off
        halloweenItem.isEnabled = !weather.enabled && modes == [.snow]
        maxPerformanceItem.state = Preferences.maxPerformance ? .on : .off
        behindWindowsItem.state = behindWindows ? .on : .off
        fullScreenItem.state = Preferences.showOverFullScreen ? .on : .off
        fullScreenItem.isEnabled = !behindWindows
        screenshotsItem.state = Preferences.showInScreenshots ? .on : .off
        loginItem.state = SMAppService.mainApp.status == .enabled ? .on : .off
    }

    private func select(intensity: Double) {
        if weather.enabled {
            weather.setManualIntensity(intensity)
            return
        }
        Preferences.intensity = intensity
        for snow in screens {
            snow.simulation.intensity = activeIntensity
            snow.setWeather(weatherScene, strength: weatherIntensity.atmosphere)
        }
        updateStatusIcon()
    }

    private func toggle(_ surface: Surfaces) {
        surfaces.formSymmetricDifference(surface)
        Preferences.surfaces = surfaces
        for snow in screens {
            snow.simulation.accumulates = !surfaces.isEmpty
            snow.simulation.groundSettles = surfaces.contains(.ground)
        }
        if surfaces.isDisjoint(with: .tracked) { screens.forEach { $0.dropWindowSnow() } }
        if let menu = statusItem.menu {
            menuNeedsUpdate(menu)
            menu.items.forEach { $0.view?.needsDisplay = true }
        }
    }

    @objc private func toggleMeltSnow() {
        thaws.toggle()
        Preferences.meltSnow = thaws
        for snow in screens { snow.simulation.thaws = thaws }
    }

    @objc private func toggleDeepInteraction() {
        Preferences.deepInteraction.toggle()
        Marquee.enabled = Preferences.deepInteraction
    }

    @objc private func clearSnow() {
        let direction = screens.first?.simulation.breezeDirection ?? -1
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        for snow in screens { snow.simulation.blowAway(direction: direction, calm: calm) }
    }

    @objc private func toggleBehindWindows() {
        behindWindows.toggle()
        Preferences.behindWindows = behindWindows
        for snow in screens { snow.window.behindWindows = behindWindows }
    }

    @objc private func toggleFullScreen() {
        Preferences.showOverFullScreen.toggle()
        for snow in screens { snow.overFullScreen = Preferences.showOverFullScreen }
    }

    @objc private func toggleScreenshots() {
        Preferences.showInScreenshots.toggle()
        for snow in screens {
            snow.window.showsInScreenshots = Preferences.showInScreenshots
            snow.dock.window.showsInScreenshots = Preferences.showInScreenshots
        }
    }

    @objc private func openSponsors() {
        NSWorkspace.shared.open(URL(string: "https://github.com/sponsors/mbezruchko")!)
    }

    @objc private func toggleOpenAtLogin() {
        let service = SMAppService.mainApp
        do {
            if service.status == .enabled {
                try service.unregister()
            } else {
                try service.register()
            }
        } catch {
            NSLog("Snowhush: login item change failed: \(error)")
        }
    }

    /// Mixed, a third mode picked takes the place of the one picked before the other.
    @objc private func chooseMode(_ sender: NSMenuItem) {
        guard let mode = modeItems.first(where: { $0.item === sender })?.mode else { return }
        if weather.enabled { weather.setEnabled(false) }
        if Preferences.mixed {
            let mix = Preferences.mix
            guard !mix.contains(mode) else { return }
            Preferences.mix = [mix[1], mode]
        } else {
            guard mode != Preferences.mode else { return }
        }
        Preferences.mode = mode
        switchModes()
    }

    /// Mixed, the mode picked last falls along with the one picked before it; unmixed, it falls alone.
    @objc private func toggleMixed() {
        if weather.enabled { weather.setEnabled(false) }
        Preferences.mixed.toggle()
        let mode = Preferences.mode
        if Preferences.mixed, !Preferences.mix.contains(mode) {
            Preferences.mix = [Preferences.mix[1], mode]
        } else if Preferences.mixed {
            Preferences.mix = [Preferences.mix.first { $0 != mode } ?? .snow, mode]
        }
        switchModes()
    }

    private func switchModes() {
        presentedModes = Preferences.modes
        refreshHalloween()
        updateStatusIcon()
        let direction = screens.first?.simulation.breezeDirection ?? -1
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        for snow in screens { snow.simulation.switchMode(to: Preferences.modes, direction: direction, calm: calm) }
    }

    @objc private func toggleMaxPerformance() {
        Preferences.maxPerformance.toggle()
        for snow in screens { snow.maxFrameRate = Preferences.maxPerformance }
    }

    @objc private func toggleHalloween() {
        Halloween.toggle()
        refreshHalloween()
    }

    private func updateStatusIcon() {
        if weather.enabled {
            let reading = weather.snapshot?.reading
            let scene = weatherScene
            let symbol = scene.modes.contains(.snow) ? "cloud.snow" : scene.modes.contains(.rain) ? "cloud.rain"
                : scene.layers.y > 0 ? "cloud.fog" : scene.layers.w > 0 ? "cloud.bolt"
                : scene.layers.z > 0 ? "sun.max" : scene.night > 0 ? "moon.stars"
                : reading?.condition?.symbol(isDay: reading?.isDay == 1) ?? "cloud"
            statusItem.button?.image = NSImage(systemSymbolName: symbol, accessibilityDescription: tr("weather.title"))
            statusItem.button?.appearsDisabled = false
        } else {
            statusItem.button?.image = Self.icon(halloween: halloween, dimmed: Preferences.intensity <= 0)
        }
        statusItem.button?.toolTip = weather.enabled ? weatherSummary : "Snowhush"
    }

    private static let symbol: [Mode: String] = [.snow: "snowflake", .autumn: "leaf", .rain: "cloud.rain"]

    /// The integrated GPU on a Mac with two: asking for the default device would wake the discrete one and hold it
    /// awake for as long as the snow falls. The devices left unused are let go before the discrete GPU powers up.
    private static func lowPowerDevice() -> MTLDevice? {
        autoreleasepool { MTLCopyAllDevices().first { $0.isLowPower } }
    }

    private static func icon(halloween: Bool, dimmed: Bool) -> NSImage? {
        let image = halloween ? Halloween.icon : Preferences.mode == .blossom ? Blossom.icon
            : NSImage(systemSymbolName: Self.symbol[Preferences.mode] ?? "snowflake", accessibilityDescription: "Snowhush")
        guard dimmed, let image else { return image }
        // Dimmed by hand: the button's appearsDisabled also disables the slider in its menu, so Off could not be left.
        let faded = NSImage(size: image.size, flipped: false) { rect in
            image.draw(in: rect, from: .zero, operation: .sourceOver, fraction: 0.4)
            return true
        }
        faded.isTemplate = image.isTemplate
        faded.accessibilityDescription = image.accessibilityDescription
        return faded
    }

    private func refreshHalloween() {
        let wanted = !weather.enabled && Halloween.isOn
        guard wanted != halloween else { return }
        halloween.toggle()
        updateStatusIcon()
        for snow in screens { snow.halloween = halloween }
    }

    // MARK: - System

    private var weatherScene: WeatherScene {
        weather.enabled ? WeatherScene.resolve(reading: weather.snapshot?.reading, effects: Preferences.weatherEffects) : .empty
    }
    private var activeModes: [Mode] {
        let modes = weatherScene.modes
        return weather.enabled ? modes.isEmpty ? [.snow] : modes : Preferences.modes
    }
    private var activeIntensity: Float {
        weather.enabled ? weatherIntensity.precipitation(for: weatherScene) : Float(Preferences.intensity)
    }
    private var weatherIntensity: WeatherIntensity {
        WeatherIntensity(automatic: Preferences.weatherAutomaticIntensity, manual: Float(Preferences.intensity))
    }

    private var weatherSummary: String {
        guard let snapshot = weather.snapshot, let condition = snapshot.reading.condition else { return tr(weather.status.key) }
        let scene = "\(snapshot.place.name) · \(Int(snapshot.reading.temperature.rounded()))° · \(tr(condition.key))"
        return weather.status == .ready ? scene : scene + " · " + tr(weather.status.key)
    }

    @objc private func toggleWeather() {
        if !weather.enabled { NSApp.activate() }
        weather.setEnabled(!weather.enabled)
    }

    @objc private func showWeather() {
        if weatherWindow == nil { weatherWindow = WeatherWindow(sync: weather) }
        weatherWindow?.show()
    }

    private func applyWeather() {
        let scene = weatherScene
        let modes = activeModes
        let change = modes != presentedModes
        presentedModes = modes
        let direction = screens.first?.simulation.breezeDirection ?? -1
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        let precipitating = scene.intensity > 0
        for snow in screens {
            if change { snow.simulation.switchMode(to: modes, direction: direction, calm: calm) }
            if weather.enabled && (!wasWeatherEnabled || wasPrecipitating && !precipitating) {
                snow.simulation.blowAway(direction: direction, calm: true)
            }
            snow.simulation.intensity = activeIntensity
            snow.setWeather(scene, strength: weatherIntensity.atmosphere)
        }
        wasWeatherEnabled = weather.enabled
        wasPrecipitating = precipitating
        refreshHalloween()
        updateStatusIcon()
        if let menu = statusItem.menu { menuNeedsUpdate(menu) }
        weatherWindow?.update()
    }

    private func observeSystem() {
        power.changed = { [unowned self] in screens.forEach { $0.apply(power.budget) } }
        let workspace = NSWorkspace.shared.notificationCenter
        let distributed = DistributedNotificationCenter.default()

        observe(workspace, NSWorkspace.screensDidSleepNotification) { $0.screensAsleep = true }
        observe(workspace, NSWorkspace.screensDidWakeNotification) { $0.screensAsleep = false }
        observe(workspace, NSWorkspace.sessionDidResignActiveNotification) { $0.sessionInactive = true }
        observe(workspace, NSWorkspace.sessionDidBecomeActiveNotification) { $0.sessionInactive = false }
        observe(distributed, Notification.Name("com.apple.screenIsLocked")) { $0.screenLocked = true }
        observe(distributed, Notification.Name("com.apple.screenIsUnlocked")) { $0.screenLocked = false }
        observe(.default, NSWindow.didChangeOcclusionStateNotification) { _ in }
        observe(.default, NSApplication.didChangeScreenParametersNotification) { $0.syncScreens() }
        observe(workspace, NSWorkspace.activeSpaceDidChangeNotification) { $0.screens.forEach { $0.spaceChanged() } }
        observe(workspace, NSWorkspace.didActivateApplicationNotification) { $0.screens.forEach { $0.watchWindows() } }
    }

    private func observe(
        _ center: NotificationCenter,
        _ name: Notification.Name,
        object: AnyObject? = nil,
        _ change: @escaping (AppDelegate) -> Void
    ) {
        observers.append(center.addObserver(forName: name, object: object, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                change(self)
                self.updateActivity()
            }
        })
    }

    private func updateActivity() {
        let awake = !screensAsleep && !screenLocked && !sessionInactive
        weather.setSuspended(!awake)
        for snow in screens {
            snow.occlusionChanged()
            snow.setActive(awake && !snow.givesWay && (snow.window.occlusionState.contains(.visible) || snow.fading), asleep: !awake)
        }
    }
}
