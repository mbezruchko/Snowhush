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
    private let cards = MenuCards()
    private let modeOrbs = MenuOrbs()
    private let extraOrbs = MenuOrbs()
    private let intensitySlider = MenuBigSlider()
    private let opacitySlider = MenuSlider(range: Preferences.opacityRange, title: tr("opacity"))
    private let windPad = MenuWindPad()
    private let windBack = MenuBack(title: tr("wind"))
    private let settleBack = MenuBack(title: tr("accumulate"))
    private let windValue = MenuNote()
    private var pages: [Page: [NSMenuItem]] = [:]
    private var page = Page.home
    private var surfaceItems: [(item: NSMenuItem, surface: Surfaces)] = []
    private var meltItem: NSMenuItem!
    private var windVariesItem: NSMenuItem!
    private var variableItem: NSMenuItem!
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

        // Laid out as Control Center is: the app's name and a switch, a tile for each thing that can be set, showing
        // what it is set to, and the intensity under them. A tile opens its page in the menu's own place, so nothing
        // flies out to the side and no window opens.
        func lay(_ page: Page, _ build: () -> Void) {
            let from = menu.items.count
            build()
            pages[page] = Array(menu.items[from...])
        }
        func back(_ row: MenuBack) {
            row.back = { [unowned self] in open(.home) }
            addView(row, to: menu)
        }

        lay(.home) {
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
            cards.cards = Array(repeating: MenuCards.Card(title: "", detail: ""), count: 4)
            cards.pressed = { [unowned self] k in open([.mode, .wind, .settle, .display][k]) }
            addView(cards, to: menu)
            intensitySlider.changed = { [unowned self] intensity in
                Preferences.paused = false
                let spread = Preferences.intensityTop > Preferences.intensity
                select(intensity: spread ? Self.spread(about: intensity) : intensity...intensity)
                refreshMenu()
            }
            addView(intensitySlider, title: tr("intensity"), to: menu)
            menu.addItem(.separator())
            clearItem = add(tr("clear.snow"), #selector(clearSnow), to: menu)

            let settings = NSMenu()
            settings.autoenablesItems = false
            settings.addItem(.sectionHeader(title: tr("intensity")))
            variableItem = toggle(tr("variable"), #selector(toggleVariableIntensity), to: settings)
            settings.addItem(.separator())
            settings.addItem(.sectionHeader(title: tr("general")))
            loginItem = toggle(tr("login"), #selector(toggleOpenAtLogin), to: settings)
            let updates = settings.addItem(withTitle: tr("updates"), action: #selector(SPUStandardUpdaterController.checkForUpdates(_:)), keyEquivalent: "")
            updates.target = updater
            menu.addItem(withTitle: tr("settings"), action: nil, keyEquivalent: "").submenu = settings
            add(tr("support"), #selector(openSponsors), to: menu).attributedTitle = Self.supportTitle
            menu.addItem(withTitle: tr("quit"), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        }

        lay(.mode) {
            back(MenuBack(title: tr("mode")))
            modeOrbs.orbs = Mode.allCases.map { MenuOrbs.Orb(name: tr("tile.\($0.key)"), image: Self.tileImage($0)) }
            modeOrbs.toggled = { [unowned self] k in choose(Mode.allCases[k]) }
            addView(modeOrbs, to: menu)
            // What goes with the modes, on the modes' own grid: each stands under one of them.
            let small = NSImage.SymbolConfiguration(pointSize: 17, weight: .regular)
            extraOrbs.columns = modeOrbs.orbs.count
            extraOrbs.orbs = [
                MenuOrbs.Orb(name: tr("mixed"), image: NSImage(systemSymbolName: "square.on.square", accessibilityDescription: nil)?.withSymbolConfiguration(small) ?? Blossom.icon),
                MenuOrbs.Orb(name: tr("halloween"), image: Halloween.icon),
            ]
            extraOrbs.toggled = { [unowned self] k in
                if k == 0 { toggleMixed() } else { spookySnow() }
                refreshMenu()
            }
            addView(extraOrbs, to: menu)
        }

        lay(.wind) {
            back(windBack)
            windBack.actionPressed = { [unowned self] in resetWind() }
            windPad.hasStop = false
            windPad.changed = { [unowned self] wind in
                Preferences.wind = wind
                for snow in screens { snow.simulation.wind = wind.vector }
                refreshMenu()
            }
            windPad.wanderChanged = { [unowned self] in wander($0) }
            addView(windPad, to: menu)
            addView(windValue, to: menu)
            addView(MenuNote(tr("hint.wind"), faint: true), to: menu)
            windVariesItem = toggle(tr("variable"), #selector(toggleWindVaries), to: menu)
        }

        // Top to bottom, as they lie on the screen; a selection is snow on windows of a sort.
        lay(.settle) {
            back(settleBack)
            settleBack.actionPressed = { [unowned self] in resetSurfaces() }
            surfaceItems = [Surfaces.widgets, .windows, .dock, .ground, .selection].map { surface in
                (toggle(SurfacePicker.name(of: surface), #selector(toggleSurfaceItem), to: menu), surface)
            }
            menu.addItem(.separator())
            meltItem = toggle(tr("melt.snow"), #selector(toggleMeltSnow), to: menu)
        }

        lay(.display) {
            back(MenuBack(title: tr("display")))
            opacitySlider.changed = { [unowned self] value in
                select(opacity: value)
                refreshMenu()
            }
            addView(opacitySlider, to: menu)
            behindWindowsItem = toggle(tr("behind"), #selector(toggleBehindWindows), to: menu)
            fullScreenItem = toggle(tr("full_screen"), #selector(toggleFullScreen), to: menu)
            screenshotsItem = toggle(tr("screenshots"), #selector(toggleScreenshots), to: menu)
            maxPerformanceItem = toggle(tr("performance"), #selector(toggleMaxPerformance), to: menu)
        }

        item.menu = menu
        return item
    }

    @discardableResult
    private func addView(_ view: NSView, title: String = "", to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: nil, keyEquivalent: "")
        item.view = view
        return item
    }

    /// What the menu shows in its one place: the tiles, or the page one of them opened.
    private enum Page {
        case home, mode, wind, settle, display
    }

    private func open(_ next: Page) {
        page = next
        refreshMenu()
    }

    private func toggle(_ title: String, _ action: Selector, to menu: NSMenu) -> NSMenuItem {
        let item = menu.addItem(withTitle: title, action: nil, keyEquivalent: "")
        let toggle = MenuToggle(action: action, target: self)
        toggle.fit(title)
        item.view = toggle
        return item
    }

    @objc private func toggleSurfaceItem(_ sender: NSMenuItem) {
        guard let surface = surfaceItems.first(where: { $0.item === sender })?.surface else { return }
        toggle(surface)
    }

    @objc private func toggleWindVaries() {
        wander(!Preferences.windVaries)
    }

    @objc private func toggleVariableIntensity() {
        let middle = (Preferences.intensity + Preferences.intensityTop) / 2
        select(intensity: Preferences.intensityTop > Preferences.intensity ? middle...middle : Self.spread(about: middle))
    }

    /// Snow settles everywhere again, and on no selection.
    private func resetSurfaces() {
        surfaces = [.ground, .dock, .windows, .widgets]
        Preferences.surfaces = surfaces
        for snow in screens {
            snow.simulation.accumulates = true
            snow.simulation.groundSettles = true
        }
        if Preferences.deepInteraction { toggleDeepInteraction() }
        refreshMenu()
    }

    /// Brings every row up to date while the menu stays open.
    private func refreshMenu() {
        guard let menu = statusItem.menu else { return }
        menuNeedsUpdate(menu)
        menu.items.forEach { $0.view?.needsDisplay = true }
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
        guard menu === statusItem?.menu else { return }
        let modes = Mode.allCases.filter(Preferences.modes.contains)
        let on = !Preferences.paused && Preferences.intensityTop > 0
        let varies = Preferences.intensityTop > Preferences.intensity
        header.isOn = on
        modeOrbs.on = Mode.allCases.map(modes.contains)
        extraOrbs.on = [Preferences.mixed, Halloween.isOn]
        intensitySlider.value = (Preferences.intensity + Preferences.intensityTop) / 2
        intensitySlider.dimmed = !on
        intensitySlider.glyph = Self.tileImage(Preferences.mode, pointSize: 11, weight: .semibold)
        variableItem.state = varies ? .on : .off
        opacitySlider.value = Preferences.fallingOpacity
        if !Preferences.windVaries { windPad.wind = Preferences.wind }
        windPad.wanders = Preferences.windVaries
        let blows = Preferences.windVaries || Preferences.wind.strength > 0
        let wind = Preferences.windVaries ? tr("variable") : Preferences.wind.summary
        windValue.text = wind
        windVariesItem.state = Preferences.windVaries ? .on : .off
        windBack.action = blows ? tr("reset") : nil

        // Rain neither heaps up nor lies about to clear away, and splashes on the ground whatever it says.
        let heaps = modes.filter { $0 != .rain }
        let settles: [Surfaces] = [.widgets, .windows, .dock] + (heaps.isEmpty ? [] : [.ground])
        var absent: [NSMenuItem] = heaps.isEmpty ? [clearItem, meltItem] : []
        for (item, surface) in surfaceItems {
            item.state = (surface == .selection ? Preferences.deepInteraction : surfaces.contains(surface)) ? .on : .off
            item.isEnabled = surface != .selection || surfaces.contains(.windows)
            if surface == .ground, heaps.isEmpty { absent.append(item) }
        }
        let lit = settles.filter(surfaces.contains)
        let everywhere: Surfaces = [.ground, .dock, .windows, .widgets]
        settleBack.action = surfaces != everywhere || Preferences.deepInteraction ? tr("reset") : nil
        clearItem.title = tr("clear", heaps)
        clearItem.isEnabled = screens.contains { $0.simulation.hasSnow || $0.simulation.spriteCount > 0 }
        meltItem.title = tr(heaps.contains(.snow) ? "melt.snow" : heaps == [.blossom] ? "melt.blossom" : "melt.autumn")
        (meltItem.view as? MenuToggle)?.fit(meltItem.title)
        meltItem.state = thaws ? .on : .off
        meltItem.isEnabled = !surfaces.isEmpty
        maxPerformanceItem.state = Preferences.maxPerformance ? .on : .off
        behindWindowsItem.state = behindWindows ? .on : .off
        fullScreenItem.state = Preferences.showOverFullScreen ? .on : .off
        fullScreenItem.isEnabled = !behindWindows
        screenshotsItem.state = Preferences.showInScreenshots ? .on : .off
        loginItem.state = SMAppService.mainApp.status == .enabled ? .on : .off

        func symbol(_ name: String) -> NSImage? {
            NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(.init(pointSize: 14, weight: .regular))
        }
        cards.cards = [
            .init(title: modes.map { tr("tile.\($0.key)") }.joined(separator: " + "), detail: Halloween.isOn ? tr("halloween") : tr("mode"),
                  image: Self.tileImage(Preferences.mode, pointSize: 14), lit: on),
            .init(title: tr("wind"), detail: wind, image: symbol("wind"), lit: blows),
            .init(title: tr("accumulate"), detail: lit.count == settles.count ? tr("all") : lit.isEmpty ? tr("off") : "\(lit.count)/\(settles.count)",
                  image: symbol("square.3.layers.3d.down.right"), lit: !lit.isEmpty),
            .init(title: tr("display"), detail: MenuSlider.percent(Preferences.fallingOpacity), image: symbol("eye"), lit: false),
        ]

        let shown = pages[page] ?? []
        for item in menu.items {
            item.isHidden = !shown.contains(item) || absent.contains(item)
        }
    }

    func menuWillOpen(_ menu: NSMenu) {
        guard menu === statusItem.menu else { return }
        windPad.showing = true
        // The menu always opens on its tiles.
        page = .home
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
