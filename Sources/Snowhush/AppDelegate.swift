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
    private let header = MenuHeader(title: "Snowhush")
    private let modeOrbs = MenuOrbs()
    private let extraOrbs = MenuOrbs()
    private var extrasItem: NSMenuItem!
    private let intensitySlider = MenuBigSlider()
    private let windRow = MenuDisclosure()
    private let windPad = MenuWindPad()
    private let windCaption = MenuDisclosure()
    private let windValue = MenuNote()
    private var clearItem: NSMenuItem!
    private let settings = SettingsController()
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

        // Laid out as the system lays a menu bar module out: its name and a switch, what is touched every day large
        // and in sight, and one row that opens Settings for what is set once and left.
        addView(header, title: "Snowhush", to: menu)
        header.toggled = { [unowned self] on in
            Preferences.paused = !on
            if on, Preferences.intensityTop <= 0 {
                Preferences.intensity = Preferences.defaultIntensity
                Preferences.intensityTop = Preferences.defaultIntensity
            }
            applyIntensity()
            refreshMenu()
        }
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
        // As wide as the modes above and no wider, whatever its own names are: the menu keeps its width when it shows.
        extraOrbs.setFrameSize(NSSize(width: modeOrbs.frame.width, height: extraOrbs.frame.height))
        extraOrbs.toggled = { [unowned self] k in
            if k == 0 { toggleMixed() } else { spookySnow() }
            refreshMenu()
        }
        extrasItem = addView(extraOrbs, to: menu)
        extrasItem.isHidden = true

        intensitySlider.changed = { [unowned self] intensity in
            Preferences.paused = false
            let spread = Preferences.intensityTop > Preferences.intensity
            select(intensity: spread ? Self.spread(about: intensity) : intensity...intensity)
            refreshMenu()
        }
        addView(intensitySlider, title: tr("intensity"), to: menu)
        menu.addItem(.separator())

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
        addView(windCaption, title: tr("wind"), to: wind)
        windCaption.foldable = false
        windCaption.open = true
        windCaption.style = .caption
        windCaption.actionPressed = { [unowned self] in resetWind() }
        addView(windPad, to: wind)
        // Under each panel's control, what it is set to and a word on what it does.
        addView(windValue, to: wind)
        addView(MenuNote(tr("hint.wind"), faint: true), to: wind)
        // Rows of our own, their value in plain grey before the chevron: a badge crowds the system's arrow.
        addView(windRow, title: tr("wind"), to: menu).submenu = wind

        clearItem = add(tr("clear.snow"), #selector(clearSnow), to: menu)
        menu.addItem(.separator())

        let open = menu.addItem(withTitle: tr("settings") + "…", action: #selector(openSettings), keyEquivalent: ",")
        open.target = self
        add(tr("support"), #selector(openSponsors), to: menu).attributedTitle = Self.supportTitle
        menu.addItem(withTitle: tr("quit"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        settings.model.groups = settingsGroups()

        item.menu = menu
        return item
    }

    @discardableResult
    private func addView(_ view: NSView, title: String = "", to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: nil, keyEquivalent: "")
        item.view = view
        return item
    }

    /// What Settings holds, top to bottom: how the weather behaves, where it settles, how it shows, and the app.
    private func settingsGroups() -> [SettingsModel.Group] {
        typealias Item = SettingsModel.Item
        func heaps() -> [Mode] { Preferences.modes.filter { $0 != .rain } }
        func check(_ title: String, _ isOn: @escaping () -> Bool, _ flip: @escaping () -> Void) -> Item {
            Item(title: { title }, kind: .toggle(isOn: isOn, flip: flip))
        }
        var surfaceRows = [Surfaces.widgets, .windows, .dock, .ground, .selection].map { surface in
            check(SurfacePicker.name(of: surface), { [unowned self] in
                surface == .selection ? Preferences.deepInteraction : surfaces.contains(surface)
            }, { [unowned self] in toggle(surface) })
        }
        // Rain splashes on the ground whatever it says; a selection is snow on windows of a sort.
        surfaceRows[3].isHidden = { heaps().isEmpty }
        surfaceRows[4].isEnabled = { [unowned self] in surfaces.contains(.windows) }
        var melt = check("", { [unowned self] in thaws }, { [unowned self] in toggleMeltSnow() })
        melt.title = { tr(heaps().contains(.snow) ? "melt.snow" : heaps() == [.blossom] ? "melt.blossom" : "melt.autumn") }
        melt.isHidden = { heaps().isEmpty }
        melt.isEnabled = { [unowned self] in !surfaces.isEmpty }
        var fullScreen = check(tr("full_screen"), { Preferences.showOverFullScreen }, { [unowned self] in toggleFullScreen() })
        fullScreen.isEnabled = { [unowned self] in !behindWindows }
        let version = (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String).map { "Snowhush \($0)" }
        return [
            .init(title: tr("variable"), items: [
                check(tr("intensity"), { Preferences.intensityTop > Preferences.intensity }, { [unowned self] in
                    let middle = (Preferences.intensity + Preferences.intensityTop) / 2
                    select(intensity: Preferences.intensityTop > Preferences.intensity ? middle...middle : Self.spread(about: middle))
                    refreshMenu()
                }),
                check(tr("wind"), { Preferences.windVaries }, { [unowned self] in wander(!Preferences.windVaries) }),
            ]),
            .init(title: tr("accumulate"), items: surfaceRows + [melt]),
            .init(title: tr("display"), items: [
                Item(title: { tr("opacity") }, kind: .slider(value: { Preferences.fallingOpacity }, set: { [unowned self] in select(opacity: $0) }, range: Preferences.opacityRange)),
                check(tr("behind"), { [unowned self] in behindWindows }, { [unowned self] in toggleBehindWindows() }),
                fullScreen,
                check(tr("screenshots"), { Preferences.showInScreenshots }, { [unowned self] in toggleScreenshots() }),
                check(tr("performance"), { Preferences.maxPerformance }, { [unowned self] in toggleMaxPerformance() }),
            ]),
            .init(title: tr("general"), items: [
                check(tr("login"), { SMAppService.mainApp.status == .enabled }, { [unowned self] in toggleOpenAtLogin() }),
                Item(title: { tr("updates") }, kind: .button { [unowned self] in updater.checkForUpdates(nil) }),
            ], footer: version),
        ]
    }

    @objc private func openSettings() {
        settings.show()
    }

    /// Brings every row up to date while the menu stays open, and Settings with it.
    private func refreshMenu() {
        guard let menu = statusItem.menu else { return }
        menuNeedsUpdate(menu)
        menu.items.forEach { $0.view?.needsDisplay = true }
        settings.model.touch()
    }

    /// A mode's tile shows the icon the menu bar shows for it.
    private static func tileImage(_ mode: Mode, pointSize: CGFloat = 17, weight: NSFont.Weight = .regular) -> NSImage {
        guard mode != .blossom, let symbol = NSImage(systemSymbolName: Self.symbol[mode] ?? "snowflake", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: pointSize, weight: weight))
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

    func menuNeedsUpdate(_ menu: NSMenu) {
        let modes = Mode.allCases.filter(Preferences.modes.contains)
        let on = !Preferences.paused && Preferences.intensityTop > 0
        header.isOn = on
        modeOrbs.on = Mode.allCases.map(modes.contains)
        extraOrbs.on = [Preferences.mixed, Halloween.isOn]
        intensitySlider.value = (Preferences.intensity + Preferences.intensityTop) / 2
        intensitySlider.dimmed = !on
        intensitySlider.glyph = Self.tileImage(Preferences.mode, pointSize: 11, weight: .semibold)
        if !Preferences.windVaries { windPad.wind = Preferences.wind }
        windPad.wanders = Preferences.windVaries
        windRow.summary = Preferences.windVaries ? tr("variable") : Preferences.wind.summary
        windRow.fit(tr("wind"), least: 0)
        windValue.text = windRow.summary
        windCaption.action = Preferences.windVaries || Preferences.wind.strength > 0 ? tr("reset") : nil
        // Rain neither heaps up nor lies about to clear away.
        let heaps = modes.filter { $0 != .rain }
        clearItem.isHidden = heaps.isEmpty
        clearItem.title = tr("clear", heaps)
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
    }

    func menuWillOpen(_ menu: NSMenu) {
        guard menu === statusItem.menu else { return }
        windPad.showing = true
        // What is on under More shows without asking: the row opens with the menu while either is.
        extrasItem.isHidden = !(Preferences.mixed || Halloween.isOn)
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

    /// Back to the default: calm, set by hand.
    private func resetWind() {
        Preferences.wind = .calm
        wander(false)
    }

    private func select(opacity: Double) {
        Preferences.fallingOpacity = opacity
        for snow in screens { snow.simulation.fallingOpacity = Float(opacity) }
    }

    /// Variable wanders a quarter either side of where the slider stands, as far as the scale lets it.
    private static func spread(about middle: Double) -> ClosedRange<Double> {
        max(0, middle - 0.25)...min(1, middle + 0.25)
    }

    /// A steady snowfall, or a range it wanders in.
    private func select(intensity range: ClosedRange<Double>) {
        Preferences.intensity = range.lowerBound
        Preferences.intensityTop = range.upperBound
        applyIntensity()
    }

    /// Has fall what is set, or nothing while switched off.
    private func applyIntensity() {
        let range = Preferences.falling
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
        statusItem.button?.image = Self.icon(halloween: halloween, dimmed: Preferences.falling.upperBound <= 0)
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
