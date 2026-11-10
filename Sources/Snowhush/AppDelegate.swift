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
    private let headerRow = MenuDisclosure()
    private let modeOrbs = MenuOrbs()
    private let extraOrbs = MenuOrbs()
    private var extrasItem: NSMenuItem!
    private let intensityRow = MenuDisclosure()
    private let intensitySlider = MenuBigSlider()
    private let opacitySlider = MenuSlider(range: Preferences.opacityRange)
    private var opacityItem: NSMenuItem!
    private var windItem: NSMenuItem!
    private let windPad = MenuWindPad()
    private var windVaryItem: NSMenuItem!
    private var windResetItem: NSMenuItem!
    private var accumulateItem: NSMenuItem!
    private var surfaceItems: [(item: NSMenuItem, surface: Surfaces)] = []
    private var behaviorItems: [NSMenuItem] = []
    private var meltItem: NSMenuItem!
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

        // Laid out as Control Center lays a module out: what is touched every day large and in sight, the rest a
        // panel to the side, so the menu never changes its height.
        addView(headerRow, title: "Snowhush", to: menu)
        headerRow.foldable = false
        headerRow.style = .heading
        if let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String { headerRow.detail = "v\(version)" }
        // After the modes, More shows or hides the small row of what goes with them.
        let more = NSImage(systemSymbolName: "ellipsis", accessibilityDescription: nil)?.withSymbolConfiguration(.init(pointSize: 17, weight: .regular)) ?? Blossom.icon
        modeOrbs.orbs = Mode.allCases.map { MenuOrbs.Orb(name: tr("tile.\($0.key)"), image: Self.tileImage($0)) } + [MenuOrbs.Orb(name: tr("more"), image: more)]
        modeOrbs.fit()
        modeOrbs.toggled = { [unowned self] k in
            guard k < Mode.allCases.count else {
                extrasItem.isHidden.toggle()
                return
            }
            choose(Mode.allCases[k])
        }
        addView(modeOrbs, to: menu)
        // What goes with the modes, on the modes' own grid: each stands under one of them.
        let small = NSImage.SymbolConfiguration(pointSize: 17, weight: .regular)
        extraOrbs.columns = modeOrbs.orbs.count
        extraOrbs.orbs = [
            MenuOrbs.Orb(name: tr("mixed"), image: NSImage(systemSymbolName: "square.on.square", accessibilityDescription: nil)?.withSymbolConfiguration(small) ?? Blossom.icon),
            MenuOrbs.Orb(name: tr("halloween"), image: Halloween.icon),
        ]
        extraOrbs.fit()
        extraOrbs.toggled = { [unowned self] k in
            if k == 0 { toggleMixed() } else { spookySnow() }
            refreshMenu()
        }
        extrasItem = addView(extraOrbs, to: menu)
        extrasItem.isHidden = true

        addView(intensityRow, title: tr("intensity"), to: menu)
        intensityRow.foldable = false
        intensityRow.open = true
        intensityRow.style = .caption
        intensityRow.pill = tr("variable")
        intensityRow.pillToggled = { [unowned self] in
            let middle = intensitySlider.value
            select(intensity: Preferences.intensityTop > Preferences.intensity ? middle...middle : Self.variableIntensity)
            refreshMenu()
        }
        intensitySlider.changed = { [unowned self] intensity in
            select(intensity: intensity...intensity)
            refreshMenu()
        }
        addView(intensitySlider, to: menu)

        let wind = NSMenu()
        wind.autoenablesItems = false
        windPad.hasStop = false
        windPad.setFrameSize(NSSize(width: 260, height: windPad.frame.height))
        windPad.changed = { [unowned self] wind in
            Preferences.wind = wind
            for snow in screens { snow.simulation.wind = wind.vector }
            refreshMenu()
        }
        windPad.wanderChanged = { [unowned self] in wander($0) }
        addView(windPad, to: wind)
        wind.addItem(.separator())
        windVaryItem = toggle(tr("variable"), #selector(toggleWindVaries), to: wind)
        windResetItem = wind.addItem(withTitle: tr("reset"), action: nil, keyEquivalent: "")
        let windReset = MenuToggle(action: #selector(resetWindItem), target: self, checks: true)
        windReset.fit(tr("reset"))
        windResetItem.view = windReset
        windItem = menu.addItem(withTitle: tr("wind"), action: nil, keyEquivalent: "")
        windItem.submenu = wind

        let opacity = NSMenu()
        opacitySlider.changed = { [unowned self] value in
            select(opacity: value)
            opacityItem.badge = NSMenuItemBadge(string: MenuSlider.percent(value))
        }
        addView(opacitySlider, to: opacity)
        opacityItem = menu.addItem(withTitle: tr("opacity"), action: nil, keyEquivalent: "")
        opacityItem.submenu = opacity

        // Top to bottom, as they lie on the screen; a selection is snow on windows of a sort.
        let accumulate = NSMenu()
        accumulate.autoenablesItems = false
        surfaceItems = [Surfaces.widgets, .windows, .dock, .ground, .selection].map { surface in
            (toggle(SurfacePicker.name(of: surface), #selector(toggleSurfaceItem), to: accumulate), surface)
        }
        accumulateItem = menu.addItem(withTitle: tr("accumulate"), action: nil, keyEquivalent: "")
        accumulateItem.submenu = accumulate
        clearItem = add(tr("clear.snow"), #selector(clearSnow), to: menu)
        menu.addItem(.separator())

        // The app itself: where the snow shows, and the rest.
        let settings = NSMenu()
        settings.autoenablesItems = false
        let behavior = NSMenuItem.sectionHeader(title: tr("behavior"))
        settings.addItem(behavior)
        meltItem = toggle(tr("melt.snow"), #selector(toggleMeltSnow), to: settings)
        let afterBehavior = NSMenuItem.separator()
        settings.addItem(afterBehavior)
        behaviorItems = [behavior, meltItem, afterBehavior]
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

    @discardableResult
    private func addView(_ view: NSView, title: String = "", to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: nil, keyEquivalent: "")
        item.view = view
        return item
    }

    /// Brings every row up to date while the menu stays open.
    private func refreshMenu() {
        guard let menu = statusItem.menu else { return }
        menuNeedsUpdate(menu)
        menu.items.forEach { $0.view?.needsDisplay = true }
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
        headerRow.fit("Snowhush", least: modeOrbs.frame.width)
        modeOrbs.on = Mode.allCases.map(modes.contains)
        let varies = Preferences.intensityTop > Preferences.intensity
        intensityRow.pillOn = varies
        intensitySlider.value = (Preferences.intensity + Preferences.intensityTop) / 2
        intensitySlider.dimmed = varies
        opacitySlider.value = Preferences.fallingOpacity
        opacityItem.badge = NSMenuItemBadge(string: MenuSlider.percent(Preferences.fallingOpacity))
        if !Preferences.windVaries { windPad.wind = Preferences.wind }
        windPad.wanders = Preferences.windVaries
        windItem.badge = NSMenuItemBadge(string: Preferences.windVaries ? tr("variable") : Preferences.wind.summary)
        windVaryItem.state = Preferences.windVaries ? .on : .off
        windResetItem.isEnabled = Preferences.windVaries || Preferences.wind.strength > 0
        extraOrbs.on = [Preferences.mixed, Halloween.isOn]
        modeOrbs.badges = Preferences.mixed || Halloween.isOn ? [Mode.allCases.count] : []
        // Rain neither heaps up nor lies about to clear away.
        let heaps = modes.filter { $0 != .rain }
        clearItem.isHidden = heaps.isEmpty
        clearItem.title = tr("clear", heaps)
        meltItem.title = tr(heaps.contains(.snow) ? "melt.snow" : heaps == [.blossom] ? "melt.blossom" : "melt.autumn")
        (meltItem.view as? MenuToggle)?.fit(meltItem.title)
        // Rain lies nowhere to melt: Behavior has nothing else in it, so it goes altogether.
        behaviorItems.forEach { $0.isHidden = heaps.isEmpty }
        // Rain splashes on the ground whatever it says.
        let settles: [Surfaces] = [.widgets, .windows, .dock] + (heaps.isEmpty ? [] : [.ground])
        for (item, surface) in surfaceItems {
            item.state = (surface == .selection ? Preferences.deepInteraction : surfaces.contains(surface)) ? .on : .off
            item.isHidden = surface == .ground && heaps.isEmpty
            item.isEnabled = surface != .selection || surfaces.contains(.windows)
        }
        let lit = settles.filter(surfaces.contains)
        let summary = lit.count == settles.count ? tr("all") : lit.isEmpty ? tr("off") : "\(lit.count)/\(settles.count)"
        accumulateItem.badge = NSMenuItemBadge(string: summary)
        meltItem.state = thaws ? .on : .off
        meltItem.isEnabled = !surfaces.isEmpty
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
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
        windPad.wanders = on
        if !on { windPad.wind = Preferences.wind }
        for snow in screens {
            snow.simulation.windWanders = on
            if !on { snow.simulation.wind = Preferences.wind.vector }
        }
        refreshMenu()
    }

    @objc private func toggleWindVaries() {
        wander(!Preferences.windVaries)
    }

    @objc private func resetWindItem() {
        resetWind()
    }

    @objc private func toggleSurfaceItem(_ sender: NSMenuItem) {
        guard let surface = surfaceItems.first(where: { $0.item === sender })?.surface else { return }
        toggle(surface)
    }


    /// Back to the default: calm, set by hand.
    private func resetWind() {
        Preferences.wind = .calm
        wander(false)
    }

    private func select(opacity: Double) {
        Preferences.fallingOpacity = opacity
        for snow in screens { snow.simulation.fallingOpacity = Float(opacity) }
    }

    /// Variable wanders about a steady fall.
    private static let variableIntensity = 0.25...0.75

    /// A steady snowfall, or a range it wanders in.
    private func select(intensity range: ClosedRange<Double>) {
        Preferences.intensity = range.lowerBound
        Preferences.intensityTop = range.upperBound
        for snow in screens {
            snow.simulation.intensity = Float(range.lowerBound)
            snow.simulation.intensityTop = Float(range.upperBound)
        }
        updateStatusIcon()
    }

    private func toggle(_ surface: Surfaces) {
        if surface == .selection {
            toggleDeepInteraction()
            refreshMenu()
            return
        }
        surfaces.formSymmetricDifference(surface)
        Preferences.surfaces = surfaces
        for snow in screens {
            snow.simulation.accumulates = !surfaces.isEmpty
            snow.simulation.groundSettles = surfaces.contains(.ground)
        }
        if surfaces.isDisjoint(with: .tracked) { screens.forEach { $0.dropWindowSnow() } }
        refreshMenu()
    }

    @objc private func toggleMeltSnow() {
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

    /// Halloween needs snow falling alone: picked over anything else, it brings the snow back with it.
    private func spookySnow() {
        guard Preferences.modes != [.snow] else {
            toggleHalloween()
            return
        }
        if !Halloween.isWanted { Halloween.toggle() }
        Preferences.mixed = false
        Preferences.mode = .snow
        switchModes()
    }

    private func toggleHalloween() {
        Halloween.toggle()
        refreshHalloween()
    }

    private func updateStatusIcon() {
        statusItem.button?.image = Self.icon(halloween: halloween, dimmed: Preferences.intensityTop <= 0)
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
