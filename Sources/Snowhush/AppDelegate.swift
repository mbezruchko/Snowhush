import AppKit
import MetalKit
import ServiceManagement
import Sparkle

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate, SPUStandardUserDriverDelegate {
    private var device: MTLDevice!
    private var screens: [ScreenSnow] = []
    private var snowfall = Snowfall.steady
    private var theme = Theme.snow
    private var themeAuto = false
    private var accumulates = true
    private var thaws = true
    private var snowOnWindows = true
    private var behindWindows = false
    private var snowfallHeld = false
    private var statusItem: NSStatusItem!
    private let snowfallSlider = SnowfallSlider()
    private var accumulateItem: NSMenuItem!
    private var meltItem: NSMenuItem!
    private var snowOnWindowsItem: NSMenuItem!
    private var clearItem: NSMenuItem!
    private var behindWindowsItem: NSMenuItem!
    private var screenshotsItem: NSMenuItem!
    private var loginItem: NSMenuItem!
    private var themeItems: [NSMenuItem] = []
    private var themeAutoItem: NSMenuItem!
    private var observers: [NSObjectProtocol] = []
    private var screensAsleep = false
    private var screenLocked = false
    private var sessionInactive = false
    private var updater: SPUStandardUpdaterController!
    private var welcome: WelcomeController?
    private let power = PowerMonitor()

    func applicationDidFinishLaunching(_ notification: Notification) {
        if AppMover.moveIfNeeded() {
            NSApp.terminate(nil)
            return
        }
        Preferences.registerDefaults()
        guard let device = MTLCreateSystemDefaultDevice() else {
            NSApp.terminate(nil)
            return
        }
        self.device = device
        snowfall = Preferences.snowfall
        theme = Preferences.theme
        themeAuto = Preferences.themeAuto
        accumulates = Preferences.accumulate
        thaws = Preferences.meltSnow
        snowOnWindows = Preferences.snowOnWindows
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
        observeSystem()
        // Auto follows the calendar; check now and then so a change of season lands without a relaunch.
        Timer.scheduledTimer(withTimeInterval: 1800, repeats: true) { [weak self] _ in
            self?.refreshSeasonalTheme()
        }
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
        let ids = Set(current.compactMap(\.displayID))
        for gone in screens where !ids.contains(gone.displayID) { gone.close() }
        screens.removeAll { !ids.contains($0.displayID) }

        for screen in current {
            if let existing = screens.first(where: { $0.displayID == screen.displayID }) {
                existing.fit(to: screen)
            } else if let snow = ScreenSnow(screen: screen, device: device, snowOnWindows: { [unowned self] in snowOnWindows }) {
                snow.simulation.snowfallHeld = snowfallHeld
                snow.simulation.snowfallReleased = { [weak self] in self?.releaseSnowfall() }
                snow.apply(power.budget)
                screens.append(snow)
            }
        }
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
        item.button?.image = NSImage(systemSymbolName: "snowflake", accessibilityDescription: "Snowhush")

        let menu = NSMenu()
        menu.delegate = self
        menu.autoenablesItems = false

        menu.addItem(.sectionHeader(title: "Snowfall"))
        snowfallSlider.changed = { [unowned self] in select($0) }
        let sliderItem = NSMenuItem()
        sliderItem.view = snowfallSlider
        menu.addItem(sliderItem)
        menu.addItem(.separator())
        let themes = NSMenu()
        for candidate in Theme.allCases {
            let item = NSMenuItem(title: candidate.title, action: #selector(pickTheme(_:)), keyEquivalent: "")
            item.target = self
            item.tag = candidate.rawValue
            item.image = NSImage(systemSymbolName: candidate.symbolName, accessibilityDescription: nil)
            themes.addItem(item)
            themeItems.append(item)
        }
        themes.addItem(.separator())
        themeAutoItem = NSMenuItem(title: "Auto (by Season)", action: #selector(pickSeasonalTheme), keyEquivalent: "")
        themeAutoItem.target = self
        themes.addItem(themeAutoItem)
        let themeRoot = NSMenuItem(title: "Theme", action: nil, keyEquivalent: "")
        themeRoot.submenu = themes
        menu.addItem(themeRoot)
        menu.addItem(.separator())
        accumulateItem = add("Accumulate", #selector(toggleAccumulate), to: menu)
        meltItem = add("Melt Snow", #selector(toggleMeltSnow), to: menu)
        snowOnWindowsItem = add("Snow on Windows", #selector(toggleSnowOnWindows), to: menu)
        clearItem = add("Clear Snow", #selector(clearSnow), to: menu)
        menu.addItem(.separator())
        behindWindowsItem = add("Behind Windows", #selector(toggleBehindWindows), to: menu)
        screenshotsItem = add("Show in Screenshots", #selector(toggleScreenshots), to: menu)
        loginItem = add("Open at Login", #selector(toggleOpenAtLogin), to: menu)
        menu.addItem(.separator())
        let updates = menu.addItem(withTitle: "Check for Updates…", action: #selector(SPUStandardUpdaterController.checkForUpdates(_:)), keyEquivalent: "")
        updates.target = updater
        menu.addItem(withTitle: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")

        item.menu = menu
        return item
    }

    @discardableResult
    private func add(_ title: String, _ action: Selector, to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: action, keyEquivalent: "")
        item.target = self
        return item
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        snowfallSlider.snowfall = snowfall
        for item in themeItems { item.state = !themeAuto && item.tag == theme.rawValue ? .on : .off }
        themeAutoItem.state = themeAuto ? .on : .off
        if themeAuto && screens.contains(where: { $0.simulation.theme != activeTheme }) {
            applyTheme()
        }
        accumulateItem.state = accumulates ? .on : .off
        accumulateItem.isEnabled = activeTheme.accumulates
        meltItem.state = thaws ? .on : .off
        meltItem.isEnabled = accumulates && activeTheme.accumulates
        snowOnWindowsItem.state = snowOnWindows ? .on : .off
        snowOnWindowsItem.isEnabled = !behindWindows
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
        behindWindowsItem.state = behindWindows ? .on : .off
        screenshotsItem.state = Preferences.showInScreenshots ? .on : .off
        loginItem.state = SMAppService.mainApp.status == .enabled ? .on : .off
    }

    /// The theme on screen: the season under Auto, otherwise the pick.
    private var activeTheme: Theme { themeAuto ? Theme.seasonal() : theme }

    @objc private func pickTheme(_ sender: NSMenuItem) {
        guard let picked = Theme(rawValue: sender.tag) else { return }
        theme = picked
        themeAuto = false
        Preferences.theme = picked
        Preferences.themeAuto = false
        applyTheme()
    }

    @objc private func pickSeasonalTheme() {
        if themeAuto {
            // Turning Auto off keeps the season that is on screen as the manual pick.
            theme = activeTheme
            Preferences.theme = theme
            themeAuto = false
            Preferences.themeAuto = false
        } else {
            themeAuto = true
            Preferences.themeAuto = true
        }
        applyTheme()
    }

    private func applyTheme() {
        let theme = activeTheme
        for snow in screens { snow.simulation.theme = theme }
    }

    private func refreshSeasonalTheme() {
        guard themeAuto, screens.contains(where: { $0.simulation.theme != activeTheme }) else { return }
        applyTheme()
    }

    private func select(_ snowfall: Snowfall) {
        self.snowfall = snowfall
        Preferences.snowfall = snowfall
        for snow in screens { snow.simulation.snowfall = snowfall }
        updateStatusIcon()
    }

    @objc private func toggleAccumulate() {
        accumulates.toggle()
        Preferences.accumulate = accumulates
        for snow in screens { snow.simulation.accumulates = accumulates }
    }

    @objc private func toggleMeltSnow() {
        thaws.toggle()
        Preferences.meltSnow = thaws
        for snow in screens { snow.simulation.thaws = thaws }
    }

    @objc private func toggleSnowOnWindows() {
        snowOnWindows.toggle()
        Preferences.snowOnWindows = snowOnWindows
        if !snowOnWindows { screens.forEach { $0.dropWindowSnow() } }
    }

    @objc private func clearSnow() {
        let direction = screens.first?.simulation.breezeDirection ?? -1
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        for snow in screens { snow.simulation.blowAway(direction: direction, calm: calm) }
    }

    @objc private func toggleBehindWindows() {
        behindWindows.toggle()
        Preferences.behindWindows = behindWindows
        for snow in screens {
            snow.window.behindWindows = behindWindows
            if behindWindows { snow.dropWindowSnow() }
        }
    }

    @objc private func toggleScreenshots() {
        Preferences.showInScreenshots.toggle()
        for snow in screens {
            snow.window.showsInScreenshots = Preferences.showInScreenshots
            snow.dock.window.showsInScreenshots = Preferences.showInScreenshots
        }
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

    private func updateStatusIcon() {
        statusItem.button?.appearsDisabled = snowfall == .off
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
            snow.setActive(awake && snow.window.occlusionState.contains(.visible))
        }
    }
}
