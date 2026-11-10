import AppKit
import MetalKit
import ServiceManagement
import Sparkle

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
    private let weatherRow = MenuDisclosure()
    private var weatherItem: NSMenuItem!
    private let modeTiles = MenuTiles()
    private let extraTiles = MenuTiles()
    private var weatherItems: [NSMenuItem] = []
    private let intensityRow = MenuDisclosure()
    private var intensityItem: NSMenuItem!
    private let intensitySlider = MenuSlider(range: 0...1, captioned: false)
    private let varySwitch = MenuSwitch()
    private let opacitySlider = MenuSlider(range: Preferences.opacityRange, title: tr("opacity"))
    private var intensityItems: [NSMenuItem] = []
    private var windItem: NSMenuItem!
    private let windRow = MenuDisclosure()
    private let windPad = MenuWindPad()
    private let windVary = MenuSwitch()
    private var windResetItem: NSMenuItem!
    private var windItems: [NSMenuItem] = []
    private var accumulateItem: NSMenuItem!
    private let accumulateRow = MenuDisclosure()
    private var surfacesItem: NSMenuItem!
    private let surfacePicker = SurfacePicker()
    private let meltSwitch = MenuSwitch()
    private var meltItem: NSMenuItem!
    private let deepSwitch = MenuSwitch()
    private var deepItem: NSMenuItem!
    private var clearItem: NSMenuItem!
    private var maxPerformanceItem: NSMenuItem!
    private var behindWindowsItem: NSMenuItem!
    private var fullScreenItem: NSMenuItem!
    private var screenshotsItem: NSMenuItem!
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
        updateStatusIcon()
        refreshHalloween()
        halloweenTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in self?.refreshHalloween() }
        observeSystem()
        Marquee.enabled = Preferences.deepInteraction
        screenshots.stepAside = { [weak self] aside in self?.screens.forEach { $0.stepAside(aside) } }
        screenshots.start()
        if snowfallHeld {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in self?.showWelcome() }
        }
    }

    // A menu bar app has no Dock icon to badge, so update reminders are left to Sparkle's own gentle alerts.
    var supportsGentleScheduledUpdateReminders: Bool { true }

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
                snow.modes = Preferences.modes
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

    private func makeStatusItem() -> NSStatusItem {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)

        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false

        // What falls, open from the start: it is what the menu is opened for most.
        weatherItem = addView(weatherRow, title: tr("weather"), to: menu)
        weatherRow.open = true
        weatherRow.pill = tr("mix")
        weatherRow.toggled = { [unowned self] in fold(weatherRow) }
        weatherRow.pillToggled = { [unowned self] in
            toggleMixed()
            refreshMenu()
        }
        modeTiles.tiles = Mode.allCases.map { MenuTiles.Tile(name: tr("tile.\($0.key)"), image: Self.tileImage($0)) }
        modeTiles.toggled = { [unowned self] in choose(Mode.allCases[$0]) }
        extraTiles.tiles = [MenuTiles.Tile(name: tr("halloween"), image: Halloween.icon)]
        extraTiles.toggled = { [unowned self] _ in
            toggleHalloween()
            refreshMenu()
        }
        let names = modeTiles.tiles.map(\.name) + extraTiles.tiles.map(\.name)
        modeTiles.fit(names)
        extraTiles.fit(names)
        weatherItems = [addView(modeTiles, to: menu), addView(extraTiles, to: menu)]

        // How much falls, and how see-through it is on the way down.
        intensityItem = addView(intensityRow, title: tr("intensity"), to: menu)
        intensityRow.summaryWhileOpen = true
        intensityRow.toggled = { [unowned self] in fold(intensityRow) }
        intensitySlider.changed = { [unowned self] in
            select(intensity: $0)
            intensityRow.summary = MenuSlider.percent($0)
        }
        varySwitch.toggled = { [unowned self] varies in
            Preferences.variesIntensity = varies
            for snow in screens { snow.simulation.variesIntensity = varies }
        }
        opacitySlider.changed = { [unowned self] opacity in
            Preferences.fallingOpacity = opacity
            for snow in screens { snow.simulation.fallingOpacity = Float(opacity) }
        }
        intensityItems = [
            addView(intensitySlider, to: menu),
            addView(varySwitch, title: tr("variable"), to: menu),
            addView(opacitySlider, to: menu),
        ]

        windItem = addView(windRow, title: tr("wind"), to: menu)
        windRow.summaryWhileOpen = true
        windRow.toggled = { [unowned self] in fold(windRow) }
        windPad.changed = { [unowned self] wind in
            Preferences.wind = wind
            for snow in screens { snow.simulation.wind = wind.vector }
            windRow.summary = wind.summary
            refreshWindReset()
        }
        windPad.stoppedWandering = { [unowned self] in wander(false) }
        windVary.toggled = { [unowned self] in wander($0) }
        let windPadItem = addView(windPad, to: menu)
        let windVaryItem = addView(windVary, title: tr("variable"), to: menu)
        let windReset = MenuToggle(action: #selector(resetWind), target: self, checks: false)
        windReset.fit(tr("reset"))
        windResetItem = addView(windReset, title: tr("reset"), to: menu)
        windItems = [windPadItem, windVaryItem, windResetItem]
        menu.addItem(.separator())

        // What lies about: where it settles, how it melts, and clearing it away.
        accumulateItem = addView(accumulateRow, title: tr("accumulate"), to: menu)
        accumulateRow.toggled = { [unowned self] in
            fold(accumulateRow)
            if !accumulateRow.open { accumulateRow.fit(accumulateItem.title, least: surfacePicker.frame.width) }
        }
        surfacePicker.toggled = { [unowned self] surface in toggle(surface) }
        surfacesItem = addView(surfacePicker, to: menu)
        meltSwitch.toggled = { [unowned self] _ in toggleMeltSnow() }
        meltItem = addView(meltSwitch, title: tr("melt.snow"), to: menu)
        deepSwitch.toggled = { [unowned self] _ in toggleDeepInteraction() }
        deepItem = addView(deepSwitch, title: tr("deep"), to: menu)
        clearItem = add(tr("clear.snow"), #selector(clearSnow), to: menu)
        menu.addItem(.separator())

        // The app itself: where the snow shows, and the rest.
        let settings = NSMenu()
        settings.autoenablesItems = false
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

        refreshFolds()
        item.menu = menu
        return item
    }

    @discardableResult
    private func addView(_ view: NSView, title: String = "", to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: nil, keyEquivalent: "")
        item.view = view
        return item
    }

    private func fold(_ row: MenuDisclosure) {
        row.open.toggle()
        refreshFolds()
    }

    /// Shows what each folded row holds while it is open; the melting switch only while something heaps up.
    private func refreshFolds() {
        weatherItems.forEach { $0.isHidden = !weatherRow.open }
        intensityItems.forEach { $0.isHidden = !intensityRow.open }
        windItems.forEach { $0.isHidden = !windRow.open }
        surfacesItem.isHidden = !accumulateRow.open
        meltItem.isHidden = !accumulateRow.open || !Preferences.modes.contains { $0 != .rain }
        deepItem.isHidden = !accumulateRow.open
    }

    /// Brings every row up to date while the menu stays open.
    private func refreshMenu() {
        guard let menu = statusItem.menu else { return }
        menuNeedsUpdate(menu)
        menu.items.forEach { $0.view?.needsDisplay = true }
    }

    /// What falls.
    private var weatherSummary: String {
        Mode.allCases.filter(Preferences.modes.contains).map { tr("tile.\($0.key)") }.joined(separator: " + ")
    }

    /// A mode's tile shows the icon the menu bar shows for it.
    private static func tileImage(_ mode: Mode) -> NSImage {
        guard mode != .blossom, let symbol = NSImage(systemSymbolName: Self.symbol[mode] ?? "snowflake", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 17, weight: .regular))
        else { return Blossom.icon }
        return symbol
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
        let modes = Mode.allCases.filter(Preferences.modes.contains)
        weatherRow.summary = weatherSummary
        weatherRow.pillOn = Preferences.mixed
        weatherRow.fit(weatherItem.title, least: modeTiles.frame.width)
        modeTiles.on = Mode.allCases.map(modes.contains)
        extraTiles.on = [Halloween.isWanted]
        extraTiles.enabled = [modes == [.snow]]
        intensityRow.summary = MenuSlider.percent(Preferences.intensity)
        intensityRow.fit(intensityItem.title, least: 0)
        intensitySlider.value = Preferences.intensity
        varySwitch.isOn = Preferences.variesIntensity
        varySwitch.fit(tr("variable"))
        opacitySlider.value = Preferences.fallingOpacity
        if !Preferences.windVaries { windPad.wind = Preferences.wind }
        windPad.wanders = Preferences.windVaries
        windVary.isOn = Preferences.windVaries
        windVary.fit(tr("variable"))
        windRow.summary = Preferences.windVaries ? tr("variable") : Preferences.wind.summary
        windRow.fit(windItem.title, least: 0)
        refreshWindReset()
        // Rain neither heaps up nor lies about to clear away.
        let heaps = modes.filter { $0 != .rain }
        clearItem.isHidden = heaps.isEmpty
        clearItem.title = tr("clear", heaps)
        meltItem.title = tr(heaps.contains(.snow) ? "melt.snow" : heaps == [.blossom] ? "melt.blossom" : "melt.autumn")
        meltSwitch.fit(meltItem.title)
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
        meltSwitch.isOn = thaws
        meltSwitch.isEnabled = !surfaces.isEmpty
        deepSwitch.isOn = Preferences.deepInteraction
        deepSwitch.isEnabled = surfaces.contains(.windows)
        deepSwitch.fit(deepItem.title)
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
        refreshFolds()
        maxPerformanceItem.state = Preferences.maxPerformance ? .on : .off
        behindWindowsItem.state = behindWindows ? .on : .off
        fullScreenItem.state = Preferences.showOverFullScreen ? .on : .off
        fullScreenItem.isEnabled = !behindWindows
        screenshotsItem.state = Preferences.showInScreenshots ? .on : .off
        loginItem.state = SMAppService.mainApp.status == .enabled ? .on : .off
    }

    func menuWillOpen(_ menu: NSMenu) {
        if menu === statusItem.menu { windPad.showing = true }
    }

    func menuDidClose(_ menu: NSMenu) {
        if menu === statusItem.menu { windPad.showing = false }
    }

    /// Lets the wind wander on its own, or has it blow as set.
    private func wander(_ on: Bool) {
        Preferences.windVaries = on
        windVary.isOn = on
        windPad.wanders = on
        if !on { windPad.wind = Preferences.wind }
        for snow in screens {
            snow.simulation.windWanders = on
            if !on { snow.simulation.wind = Preferences.wind.vector }
        }
        windRow.summary = on ? tr("variable") : Preferences.wind.summary
        refreshWindReset()
    }

    /// Back to the default: calm, set by hand.
    @objc private func resetWind() {
        Preferences.wind = .calm
        wander(false)
    }

    /// Reset is there only while the wind is off its default.
    private func refreshWindReset() {
        windResetItem.isEnabled = Preferences.windVaries || Preferences.wind.strength > 0
        windResetItem.view?.needsDisplay = true
    }

    private func select(intensity: Double) {
        Preferences.intensity = intensity
        for snow in screens { snow.simulation.intensity = Float(intensity) }
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
        refreshMenu()
    }

    private func toggleMeltSnow() {
        thaws.toggle()
        Preferences.meltSnow = thaws
        for snow in screens { snow.simulation.thaws = thaws }
    }

    private func toggleDeepInteraction() {
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
    private func choose(_ mode: Mode) {
        if Preferences.mixed {
            let mix = Preferences.mix
            guard !mix.contains(mode) else { return }
            Preferences.mix = [mix[1], mode]
        } else {
            guard mode != Preferences.mode else { return }
        }
        Preferences.mode = mode
        switchModes()
        refreshMenu()
    }

    /// Mixed, the mode picked last falls along with the one picked before it; unmixed, it falls alone.
    private func toggleMixed() {
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

    private func toggleHalloween() {
        Halloween.toggle()
        refreshHalloween()
    }

    private func updateStatusIcon() {
        statusItem.button?.image = Self.icon(halloween: halloween, dimmed: Preferences.intensity <= 0)
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
        guard Halloween.isOn != halloween else { return }
        halloween.toggle()
        updateStatusIcon()
        for snow in screens { snow.halloween = halloween }
    }

    // MARK: - System

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
            guard let self else { return }
            change(self)
            self.updateActivity()
        })
    }

    private func updateActivity() {
        let awake = !screensAsleep && !screenLocked && !sessionInactive
        for snow in screens {
            snow.occlusionChanged()
            snow.setActive(awake && !snow.givesWay && (snow.window.occlusionState.contains(.visible) || snow.fading), asleep: !awake)
        }
    }
}
