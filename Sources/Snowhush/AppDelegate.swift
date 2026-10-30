import AppKit
import MetalKit
import ServiceManagement
import Sparkle

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate, SPUStandardUserDriverDelegate {
    private var device: MTLDevice!
    private var screens: [ScreenSnow] = []
    private var snowfall = Snowfall.steady
    private var accumulates = true
    private var thaws = true
    private var snowOnWindows = true
    private var behindWindows = false
    private var snowfallHeld = false
    private var halloween = false
    private var statusItem: NSStatusItem!
    private let snowfallSlider = SnowfallSlider()
    private var accumulateItem: NSMenuItem!
    private var meltItem: NSMenuItem!
    private var snowOnWindowsItem: NSMenuItem!
    private var clearItem: NSMenuItem!
    private var halloweenItem: NSMenuItem!
    private var behindWindowsItem: NSMenuItem!
    private var screenshotsItem: NSMenuItem!
    private var loginItem: NSMenuItem!
    private var observers: [NSObjectProtocol] = []
    private var screensAsleep = false
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
        guard let device = MTLCreateSystemDefaultDevice() else {
            NSApp.terminate(nil)
            return
        }
        self.device = device
        snowfall = Preferences.snowfall
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
        refreshHalloween()
        halloweenTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in self?.refreshHalloween() }
        observeSystem()
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
                snow.halloween = halloween
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
        accumulateItem = add("Accumulate", #selector(toggleAccumulate), to: menu)
        meltItem = add("Melt Snow", #selector(toggleMeltSnow), to: menu)
        snowOnWindowsItem = add("Snow on Windows", #selector(toggleSnowOnWindows), to: menu)
        clearItem = add("Clear Snow", #selector(clearSnow), to: menu)
        halloweenItem = add("Halloween", #selector(toggleHalloween), to: menu)
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
        accumulateItem.state = accumulates ? .on : .off
        meltItem.state = thaws ? .on : .off
        meltItem.isEnabled = accumulates
        snowOnWindowsItem.state = snowOnWindows ? .on : .off
        snowOnWindowsItem.isEnabled = !behindWindows
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
        halloweenItem.isHidden = !Halloween.isSeason()
        halloweenItem.state = Preferences.halloween ? .on : .off
        behindWindowsItem.state = behindWindows ? .on : .off
        screenshotsItem.state = Preferences.showInScreenshots ? .on : .off
        loginItem.state = SMAppService.mainApp.status == .enabled ? .on : .off
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

    @objc private func toggleHalloween() {
        Preferences.halloween.toggle()
        refreshHalloween()
    }

    private func updateStatusIcon() {
        statusItem.button?.appearsDisabled = snowfall == .off
    }

    private func refreshHalloween() {
        guard Halloween.isOn != halloween else { return }
        halloween.toggle()
        statusItem.button?.image = halloween ? Halloween.icon : NSImage(systemSymbolName: "snowflake", accessibilityDescription: "Snowhush")
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
            snow.setActive(awake && snow.window.occlusionState.contains(.visible))
        }
    }
}
