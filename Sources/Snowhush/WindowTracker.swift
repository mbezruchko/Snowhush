import AppKit

struct WindowUpdate {
    let surfaces: [WindowSurface]
    /// Seconds since the previous poll.
    let interval: Float
    /// Windows that left the list but still exist off screen, where it can't be told whether they were hidden
    /// or left behind on another Space.
    let parked: Set<CGWindowID>
    /// Windows left behind on another Space: their snow waits for them.
    let elsewhere: Set<CGWindowID>
    /// The whole Space is sliding past, as on a switch between Spaces: windows move without being moved.
    let sliding: Bool
    /// How fast the sliding Space carried its windows since the previous poll, in points a second, negative to
    /// the left; zero when it didn't.
    let slideVelocity: Float
    /// Windows that moved on to another display, which their snow goes to, by that display.
    let handed: [CGWindowID: CGDirectDisplayID]
}

struct WindowSurface: Equatable {
    let id: CGWindowID
    /// Overlay coordinates: origin at the bottom-left of the primary screen.
    let frame: CGRect
    /// Frames of windows in front of this one near its top edge, in overlay coordinates.
    let blockers: [CGRect]
    /// Frames of windows in front of any part of this one: what hides a drop running down it.
    let cover: [CGRect]
    /// Top corners as a superellipse quadrant: macOS windows are close to exponent 2.5, Spotlight's capsule is a circle.
    let cornerRadius: CGFloat
    let cornerExponent: CGFloat
    /// The Dock's shelf: its snow is drawn below the Dock, so the icons stay in front of it.
    let isDock: Bool
    /// A notification banner sliding in over the windows: it pushes the snow in its way aside.
    let sweeps: Bool

    func shifted(by dx: CGFloat) -> WindowSurface {
        WindowSurface(
            id: id,
            frame: frame.offsetBy(dx: dx, dy: 0),
            blockers: blockers.map { $0.offsetBy(dx: dx, dy: 0) },
            cover: cover.map { $0.offsetBy(dx: dx, dy: 0) },
            cornerRadius: cornerRadius,
            cornerExponent: cornerExponent,
            isDock: isDock,
            sweeps: sweeps
        )
    }
}

/// Polls the on-screen window list: a few times a second when nothing moves, every frame while something does.
final class WindowTracker {
    /// The display tracked, in the window list's global coordinates: origin at the top left of the primary display.
    var screen: CGRect = .zero
    var display: CGDirectDisplayID?
    var menuBarHeight: CGFloat = 0
    /// The primary display's height, which turns AppKit's coordinates into the window list's.
    var primaryHeight: CGFloat = 0
    /// Which display each window's snow lies on.
    private let displays: Displays

    private var currentSpace: UInt64?
    private var currentSpaceRead = false
    private let ownPID = ProcessInfo.processInfo.processIdentifier
    private let maxLayer = Int(CGWindowLevelForKey(.screenSaverWindow))
    private let dockLevel = Int(CGWindowLevelForKey(.dockWindow))
    private let desktopIconLevel = Int(CGWindowLevelForKey(.desktopIconWindow))
    /// Whether each process seen with a window just above the desktop's icons is Notification Center, which shows
    /// the desktop's widgets there.
    private var widgetHosts: [Int32: Bool] = [:]
    /// Whether each process seen with a window above the normal layer is the one that shows Picture in Picture.
    private var pipHosts: [Int32: Bool] = [:]
    /// CursorUIViewService draws the input source badge next to the text caret.
    private let ignoredOwners: Set<String> = ["Window Server", "CursorUIViewService"]
    /// The Dock's windows (the Dock itself, the ⌘Tab switcher) hide snow behind them but don't collect any.
    private let coverOnlyOwners: Set<String> = ["Dock"]
    private var previous: [WindowSurface] = []
    private var lastQuery: CFTimeInterval = 0
    private var lastChange: CFTimeInterval = -10
    /// When an auto-hidden Dock came out; nil while it is away.
    private var dockShownAt: CFTimeInterval?
    /// Whether the Dock showing on this display hides by itself, so it is watched closely for when it starts to.
    private var dockHides = false
    /// Whether the Dock is out on this display and stays out, so it is watched for being turned to hide by itself.
    private var dockStays = false
    /// Whether the last look found the Dock's shelf out on this display.
    private var dockWasOut = false
    /// The Dock's window while a full-screen app keeps the Dock away.
    private var sinkingDock: CGWindowID?
    /// A full-screen app shows on this display, from the moment its Space starts sliding in.
    private(set) var fullScreenInView = false
    /// How much of this display a full-screen app's Space covers, from 0 to 1, as it slides in or away.
    private(set) var fullScreenShare: CGFloat = 0
    private var alphas: [CGWindowID: Double] = [:]
    /// Windows on their way out before the window list shows it, as during a minimize's genie animation;
    /// left out until they're gone or their time is up.
    private var leaving: [CGWindowID: CFTimeInterval] = [:]
    /// The radius the window server rounds each window's corners with, learnt as the window shows up.
    private var radii: [CGWindowID: CGFloat] = [:]
    /// A window's corners when the window server can't tell: from macOS 27 every window has the same, tighter ones.
    private static let defaultRadius: CGFloat = {
        if #available(macOS 27, *) { return 16 }
        return 18
    }()
    /// A desktop widget's corners, which the window server doesn't round: measured off the widgets themselves, as the
    /// superellipse the snow's bed takes. From macOS 27 they round off further.
    private static let widgetRadius: CGFloat = {
        if #available(macOS 27, *) { return 31 }
        return 22
    }()
    /// Where each window above the normal layer last showed smaller than the screen, and when.
    private var raised: [CGWindowID: (frame: CGRect, at: CFTimeInterval)] = [:]
    private var pressedMinimize: CGWindowID?
    private var buttonDown = false
    /// When a modifier of a window shortcut was last seen held.
    private var windowKeysAt: CFTimeInterval = -10
    private var slidingUntil: CFTimeInterval = -10
    /// The largest window at the last poll, checked every frame until the next: a Space that starts sliding carries
    /// it off well before the next poll would see. A floating panel might stay on every Space; a window that big won't.
    private var watched: (id: CGWindowID, frame: CGRect)?
    /// Every app window on the screen at the last poll, by id, those that carry no snow too: a Space slides them all.
    private var windowFrames: [CGWindowID: CGRect] = [:]
    /// A Space sliding on by itself after a swipe, for the snow to keep up with what the window list reports late.
    private var glide = Glide()
    /// How far ahead of the window list the last update drew the sliding Space's snow.
    private var lead: CGFloat = 0
    /// Windows that stay put while the Space slides, like panels on every Space: no lead for them.
    private var staying: Set<CGWindowID> = []
    /// Mission Control or App Exposé shows every window shrunk, away from where the window list puts it.
    private var exposing = false
    private var banner = NotificationBanner()
    /// The level of Notification Center's window while it is up, showing banners or opened as a whole.
    var notificationLevel: Int? { banner.level }
    /// Whether the pointer is over Notification Center's panel or a banner, in front of the snow it can't reach there.
    var notificationCoversPointer: Bool {
        guard banner.level != nil else { return false }
        let pointer = pointer
        if var panel = banner.panelFrame(screen: screen, menuBarHeight: menuBarHeight) {
            // The panel ends above a Dock along the bottom edge, which keeps the snow heaped on it within reach.
            if let reserved = Dock.reserved, abs(reserved.maxY - screen.maxY) < 2 {
                panel.size.height = screen.maxY - max(reserved.height, Dock.height) - 40 - panel.minY
            }
            if panel.contains(pointer) { return true }
        }
        return banner.surfaces(now: CACurrentMediaTime(), screen: screen, menuBarHeight: menuBarHeight).contains { $0.frame.contains(pointer) }
    }
    /// When each floating panel carrying snow (Spotlight and the like) first showed.
    private var panelsShownAt: [CGWindowID: CFTimeInterval] = [:]
    /// Every app window on the screen at the last poll, in overlay coordinates, those filling it or too tall to
    /// carry snow too.
    private(set) var covers: [CGRect] = []
    /// Windows carrying snow here at the last poll that have moved on to another display since.
    private var handed: [CGWindowID: CGDirectDisplayID] = [:]
    private var cards: [CGWindowID: (bounds: CGRect, card: Bool)] = [:]

    init(displays: Displays) {
        self.displays = displays
    }

    /// Returns the windows when they changed since the last poll.
    /// `eager` polls every frame, e.g. while a mouse button is down and a drag may be starting.
    func poll(now: CFTimeInterval, eager: Bool) -> WindowUpdate? {
        let interval = now - lastQuery
        let minimizing = watchForMinimize(now: now)
        let bannerMoving = banner.update(now: now, screen: screen, menuBarHeight: menuBarHeight, pointer: pointer)
        // Panels like Spotlight vanish on a keystroke; once one has stayed a while it's more likely a long-lived floating window.
        let panelOpen = panelsShownAt.values.contains { now - $0 < 20 }
        // Even while windows move, 60 looks a second are plenty: the snow follows a smoothed path of the window anyway.
        let closely = eager || now - lastChange < 0.6 || dockShownAt != nil || dockHides || pointerAtBottomEdge || panelOpen || exposing || windowsMayChange(now: now)
        // A sliding Space is followed every frame, its snow riding on the windows with no smoothing.
        // Turned to hide by itself, a Dock that stayed out slides away at once: the slow polls would let its snow hang.
        let dockTurnedToHide = dockStays && Dock.autoHides
        guard minimizing || bannerMoving || now < slidingUntil || interval >= (closely ? 1.0 / 62 : 0.2) || dockTurnedToHide || watchedMoved() else { return nil }
        lastQuery = now
        let before = windowFrames
        let surfaces = query(now: now)
        watched = windowFrames.max { $0.value.width * $0.value.height < $1.value.width * $1.value.height }.map { ($0.key, $0.value) }
        let shift = slideShift(from: before, to: windowFrames)
        if shift != nil {
            slidingUntil = now + 0.3
            staying = Set(surfaces.filter { surface in previous.contains { $0.id == surface.id && $0.frame == surface.frame } }.map(\.id))
        }
        let sliding = now < slidingUntil
        let ahead: CGFloat
        if sliding {
            if let watched { probe(watched) }
            // Spaces slide past a screen's width and a 64-point gap apart.
            ahead = glide.lead(now: now, shift: shift, toRest: distancesToRest(of: surfaces), pitch: Double(screen.width) + 64)
        } else {
            glide = Glide()
            ahead = 0
        }
        // Snow drawn ahead of a Space that has just stopped still has to settle back onto its windows.
        guard surfaces != previous || ahead != lead || shift != nil else { return nil }
        lead = ahead
        let vanished = Set(previous.map(\.id)).subtracting(surfaces.map(\.id))
        let dockGone = vanished.intersection(previous.filter(\.isDock).map(\.id))
        previous = surfaces
        lastChange = now
        let away = offscreen(among: vanished)
        // Windows carried off the screen by a sliding Space or shrunk by Mission Control are still listed,
        // but their snow should wait for them too.
        var elsewhere = sliding || exposing ? away.elsewhere.union(vanished.subtracting(away.parked)) : away.elsewhere
        // The Dock stays behind as a full-screen app's Space slides in, and its snow falls.
        if let dock = sinkingDock { elsewhere.remove(dock) }
        // A Dock that hides or moves to another display drops its snow at once: its snow waits for it only while
        // Mission Control shows or the Space slides.
        var parked = away.parked
        if !sliding && !exposing && sinkingDock == nil {
            elsewhere.subtract(dockGone)
            parked.subtract(dockGone)
        }
        let shown = lead == 0 ? surfaces : surfaces.map { $0.isDock || NotificationBanner.ids.contains($0.id) || staying.contains($0.id) ? $0 : $0.shifted(by: lead) }
        let slideVelocity = shift.map { Float($0) / Float(max(interval, 1.0 / 120)) } ?? 0
        return WindowUpdate(
            surfaces: shown,
            interval: Float(min(interval, 0.25)),
            parked: parked,
            elsewhere: elsewhere,
            sliding: sliding,
            slideVelocity: slideVelocity,
            handed: handed
        )
    }

    /// How far each window carried by the sliding Space is from where it rests on its own Space: the window server keeps
    /// that frame through the slide, while the window list follows the window across the screen.
    private func distancesToRest(of surfaces: [WindowSurface]) -> [Double] {
        surfaces.filter { !$0.isDock && !NotificationBanner.ids.contains($0.id) && !staying.contains($0.id) }.prefix(4).compactMap { surface in
            SkyLight.bounds(of: surface.id).map { Double($0.minX - screen.minX - surface.frame.minX) }
        }
    }

    /// Looks at the watched window again a moment after the poll: a Space sliding on by itself moves its windows on
    /// within a few milliseconds, while one carried by fingers keeps them till the next frame.
    private func probe(_ watched: (id: CGWindowID, frame: CGRect)) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.006) { [weak self] in
            guard let self, let bounds = Self.listedBounds(of: watched.id) else { return }
            glide.noteMovedSoon(abs(bounds.minX - screen.minX - watched.frame.minX) > 0.5)
        }
    }

    /// How far every window that moved slid sideways, by the same amount and unchanged otherwise, with no button held:
    /// nobody drags several windows at once, but a Space switch carries them all. Nil when they didn't.
    private func slideShift(from before: [CGWindowID: CGRect], to now: [CGWindowID: CGRect]) -> CGFloat? {
        guard NSEvent.pressedMouseButtons == 0 else { return nil }
        var shift: CGFloat?
        for (id, frame) in now {
            guard let old = before[id], old != frame else { continue }
            let dx = frame.minX - old.minX
            guard frame.size == old.size, abs(frame.minY - old.minY) < 0.5, abs(dx - (shift ?? dx)) < 1 else { return nil }
            shift = dx
        }
        return shift
    }

    /// Catches a minimize as it starts, from the click on the window's minimize button or ⌘M: the window list
    /// shows nothing until the genie animation is over. Returns whether a window just started leaving.
    private func watchForMinimize(now: CFTimeInterval) -> Bool {
        if !leaving.isEmpty { leaving = leaving.filter { $0.value > now } }
        var started = false
        let down = NSEvent.pressedMouseButtons & 1 != 0
        if down && !buttonDown {
            pressedMinimize = minimizeButton(at: pointer, now: now)
        } else if !down && buttonDown, let id = pressedMinimize {
            if minimizeButton(at: pointer, now: now) == id {
                leaving[id] = now + 1
                started = true
            }
            pressedMinimize = nil
        }
        buttonDown = down

        let flags = CGEventSource.flagsState(.combinedSessionState)
        if !flags.isDisjoint(with: Self.windowKeys) { windowKeysAt = now }
        // 0x2E is the M key.
        if flags.contains(.maskCommand), CGEventSource.keyState(.combinedSessionState, key: 0x2E),
           let app = NSWorkspace.shared.frontmostApplication?.processIdentifier {
            let windows = Self.windowList(now: now).filter { $0.pid == app && $0.layer == 0 }
            // ⌥⌘M minimizes all of the app's windows.
            for entry in flags.contains(.maskAlternate) ? windows : Array(windows.prefix(1)) {
                guard let id = entry.id, leaving[id] == nil else { continue }
                leaving[id] = now + 1
                started = true
            }
        }
        return started
    }

    /// The snowy window whose minimize button is under the pointer. The buttons sit in a row 23 points apart, the
    /// close button as far from the window's left edge as from its top, however tall the title bar.
    private func minimizeButton(at point: CGPoint, now: CFTimeInterval) -> CGWindowID? {
        let front = Self.windowList(now: now).first { entry in
            guard let layer = entry.layer, layer >= 0, layer < maxLayer, entry.pid != ownPID,
                  (entry.alpha ?? 0) > 0.3, let bounds = entry.bounds
            else { return false }
            return bounds.contains(point)
        }
        guard let front, let id = front.id, previous.contains(where: { $0.id == id }), let bounds = front.bounds else { return nil }
        let dx = point.x - bounds.minX
        let dy = point.y - bounds.minY
        return (9...33).contains(dy) && abs(dx - dy - 23) <= 7 ? id : nil
    }

    /// The modifiers of the shortcuts that close, hide, minimize or zoom windows (⌘W, ⌘H, ⌘M, ⌃⌘F, fn F); Shift and
    /// Option alone, pressed all along in typing, change none.
    private static let windowKeys: CGEventFlags = [.maskCommand, .maskControl, .maskSecondaryFn]

    /// A click just ended or a window shortcut's modifier was let go: a window may be closing (its close button, ⌘W),
    /// minimizing or hiding, and its snow should fall the moment it does.
    private func windowsMayChange(now: CFTimeInterval) -> Bool {
        now - windowKeysAt < 0.5 || CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: .leftMouseUp) < 0.5
    }

    /// Polls every frame for a moment, for changes the mouse doesn't announce (app switching, Spaces).
    func watchClosely(now: CFTimeInterval) {
        lastChange = now
    }

    func reset() {
        previous = []
        windowFrames = [:]
        watched = nil
        covers = []
    }

    /// Whether the watched window left the place the last poll found it, by a look at it alone: a fraction of the
    /// cost of the whole window list, and like the list, it follows the window as its Space slides.
    private func watchedMoved() -> Bool {
        guard let watched, let bounds = Self.listedBounds(of: watched.id) else { return false }
        return abs(bounds.minX - screen.minX - watched.frame.minX) > 0.5 || abs(screen.maxY - bounds.maxY - watched.frame.minY) > 0.5
    }

    /// Where the window list puts one window, in its own coordinates.
    private static func listedBounds(of window: CGWindowID) -> CGRect? {
        // CGWindowListCreateDescriptionFromArray takes window numbers stored as raw pointer values.
        var pointer = UnsafeRawPointer(bitPattern: UInt(window))
        guard let array = CFArrayCreate(nil, &pointer, 1, nil) else { return nil }
        return ListedWindow.all(in: CGWindowListCreateDescriptionFromArray(array)).first?.bounds
    }

    /// Sorts windows that left the list: closed ones and ones hidden on the current Space are in neither set.
    private func offscreen(among ids: Set<CGWindowID>) -> (parked: Set<CGWindowID>, elsewhere: Set<CGWindowID>) {
        guard !ids.isEmpty else { return ([], []) }
        // CGWindowListCreateDescriptionFromArray takes window numbers stored as raw pointer values.
        var pointers: [UnsafeRawPointer?] = ids.filter { $0 != 0 }.map { UnsafeRawPointer(bitPattern: UInt($0)) }
        guard let array = CFArrayCreate(nil, &pointers, pointers.count, nil) else { return ([], []) }
        var away: [CGWindowID] = []
        var parked = Set<CGWindowID>()
        var elsewhere = Set<CGWindowID>()
        for entry in ListedWindow.all(in: CGWindowListCreateDescriptionFromArray(array)) {
            guard let id = entry.id else { continue }
            // Gone full screen or zoomed to fill it, a window has no room for snow; its snow waits for it to come back to size.
            // Going full screen, it is off screen for the move to its own Space but already has its full-screen size.
            // Only app windows: the Dock's window spans the screen too, and its snow falls when it hides.
            if entry.layer == 0, let bounds = entry.bounds, fillsScreen(bounds) {
                elsewhere.insert(id)
            } else if entry.onScreen != true {
                away.append(id)
            }
        }
        let current = Spaces.current()
        for id in away {
            // A Space switch makes the new Space current before the old one's windows leave the screen.
            guard let current, let spaces = Spaces.of(id) else {
                parked.insert(id)
                continue
            }
            if spaces.isDisjoint(with: current) { elsewhere.insert(id) }
        }
        return (parked, elsewhere)
    }

    /// Whether a window, in the window list's coordinates, covers this display below the menu bar.
    private func fillsScreen(_ bounds: CGRect) -> Bool {
        bounds.intersects(screen) && bounds.width * bounds.height >= screen.width * (screen.height - menuBarHeight) * 0.9
    }

    /// Where the pointer reveals an auto-hidden Dock.
    private var pointerAtBottomEdge: Bool {
        pointer.y >= screen.maxY - 3 && pointer.x >= screen.minX && pointer.x < screen.maxX
    }

    /// The pointer in the window list's coordinates.
    private var pointer: CGPoint {
        let location = NSEvent.mouseLocation
        return CGPoint(x: location.x, y: primaryHeight - location.y)
    }

    /// The Dock's shelf while it is out, nil while it is away or leaving. `window` is the Dock's window.
    private func dockShelf(now: CFTimeInterval, window: CGRect) -> CGRect? {
        dockStays = false
        guard let room = Dock.room(on: screen, window: window) else {
            dockShownAt = nil
            return nil
        }
        if room.height > 0 { Dock.keep(room.height) }
        // An auto-hidden Dock mostly keeps no room even while it is out, but at times keeps all of it until it starts
        // to hide.
        guard room.height == 0 || Dock.autoHides else {
            dockShownAt = nil
            dockHides = false
            dockStays = true
            return Dock.shelf(in: room)
        }
        // Auto-hidden: it stays out while the pointer is in a band along the bottom of the screen, a little taller than
        // the Dock and as wide as the screen, and starts sliding away the moment the pointer leaves it. Its window
        // stays listed until it has slid out, a quarter of a second later.
        dockHides = true
        let height = room.height > 0 ? room.height : Dock.height
        let shelf = Dock.shelf(in: CGRect(x: room.minX, y: screen.maxY - height, width: room.width, height: height))
        let pointer = pointer
        guard pointer.y >= screen.maxY - height - Dock.holdMargin, pointer.x >= screen.minX, pointer.x < screen.maxX else {
            dockShownAt = nil
            return nil
        }
        // Turned to hide by itself under the pointer, the Dock stays where it is rather than rising again.
        let shownAt = dockShownAt ?? (dockWasOut ? now - Dock.revealTime : now)
        dockShownAt = shownAt
        let progress = min(1, (now - shownAt) / Dock.revealTime)
        let rise = 1 - pow(1 - progress, 3)
        return shelf.offsetBy(dx: 0, dy: (screen.maxY - shelf.minY) * (1 - rise))
    }

    /// How much of this display a full-screen app's Space covers, read from the window list alone, without a poll.
    func readFullScreenShare(now: CFTimeInterval) -> CGFloat {
        fullScreenShare(in: Self.windowList(now: now))
    }

    /// Whether Mission Control or App Exposé shows on this display.
    func missionControlShows(now: CFTimeInterval) -> Bool {
        Self.windowList(now: now).contains { entry in
            guard entry.owner == "WindowManager", let layer = entry.layer, layer > 0, let bounds = entry.bounds else { return false }
            return isMissionControl(bounds, layer: layer)
        }
    }

    /// Mission Control and App Exposé lay a shield just below the Dock, and Mission Control keeps its bar of Spaces
    /// along the top, which stays while the shield drops out. Show Desktop and Spotlight use a lower full-screen window.
    private func isMissionControl(_ bounds: CGRect, layer: Int) -> Bool {
        guard bounds.intersects(screen) else { return false }
        let shield = layer >= dockLevel - 1 && bounds.width * bounds.height >= screen.width * screen.height * 0.9
        let bar = bounds.width >= screen.width * 0.9 && bounds.minY <= screen.minY + 1 && bounds.height < screen.height * 0.3
        return shield || bar
    }

    private func fullScreenShare(in list: [ListedWindow]) -> CGFloat {
        let candidates = list.compactMap { entry -> (id: CGWindowID, bounds: CGRect)? in
            guard entry.layer == 0, let id = entry.id, let bounds = entry.bounds, fillsScreen(bounds) else { return nil }
            return (id, bounds)
        }
        guard !candidates.isEmpty, let fullScreen = Spaces.fullScreen(), !fullScreen.isEmpty else { return 0 }
        return candidates.reduce(0) { share, candidate in
            guard let spaces = Spaces.of(candidate.id), !spaces.isDisjoint(with: fullScreen) else { return share }
            return max(share, candidate.bounds.intersection(screen).width / screen.width)
        }
    }

    /// How much of this display desktops cover while a full-screen app's Space is current, from 0 to 1, read from the
    /// window list alone: what the app's windows leave of it as they slide away, or what a desktop's windows show of
    /// it as they slide in, whichever is more; nil with neither in sight. A desktop may have no windows, and an app's
    /// window may stay put in the list as its Space slides.
    func desktopShare(now: CFTimeInterval) -> CGFloat? {
        guard let fullScreen = Spaces.fullScreen() else { return nil }
        var apps: [UInt64: CGFloat] = [:]
        var desktops: [UInt64: CGFloat] = [:]
        let area = screen.width * (screen.height - menuBarHeight) * 0.9
        for entry in Self.windowList(now: now) {
            guard entry.layer == 0, entry.pid != ownPID, let id = entry.id, let listed = entry.bounds else { continue }
            let large = listed.width * listed.height >= area
            let inView = listed.intersects(screen)
            guard large || inView, let spaces = Spaces.of(id), let space = spaces.first else { continue }
            if !spaces.isDisjoint(with: fullScreen) {
                guard large else { continue }
                apps[space] = max(apps[space] ?? 0, listed.intersection(screen).width / screen.width)
            } else if inView {
                // The list shows the window sliding in with its Space; its frame on the Space stays put.
                let shown = SkyLight.bounds(of: id).map { max(0, 1 - abs(listed.minX - $0.minX) / screen.width) } ?? 1
                desktops[space] = max(desktops[space] ?? 0, shown)
            }
        }
        guard !apps.isEmpty || !desktops.isEmpty else { return nil }
        let left = apps.isEmpty ? 0 : 1 - min(1, apps.values.reduce(0, +))
        return max(left, min(1, desktops.values.reduce(0, +)))
    }

    /// Whether an app in full screen shows on this display, its window reaching down past the Dock: the Dock sinks away
    /// as soon as the app's Space starts sliding in and stays away there, though the window server still reports it out.
    private func fullScreenAppHidesDock(in list: [ListedWindow]) -> Bool {
        let band = CGRect(x: screen.minX, y: screen.maxY - Dock.height, width: screen.width, height: Dock.height)
        let candidates = list.compactMap { entry -> CGWindowID? in
            guard entry.layer == 0, let bounds = entry.bounds, bounds.intersects(band), fillsScreen(bounds) else { return nil }
            return entry.id
        }
        guard !candidates.isEmpty, let fullScreen = Spaces.fullScreen(), !fullScreen.isEmpty else { return false }
        return candidates.contains { Spaces.of($0).map { !$0.isDisjoint(with: fullScreen) } ?? false }
    }

    private static var shared: (time: CFTimeInterval, list: [ListedWindow])?

    /// One window list serves every display's tracker that polls within the same frame.
    private static func windowList(now: CFTimeInterval) -> [ListedWindow] {
        if let shared, abs(now - shared.time) < 0.008 { return shared.list }
        let list = ListedWindow.all(in: CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID))
        shared = (now, list)
        return list
    }

    private func query(now: CFTimeInterval) -> [WindowSurface] {
        guard screen.width > 0 else { return [] }
        let list = Self.windowList(now: now)
        currentSpaceRead = false
        raised = raised.filter { now - $0.value.at < 2 }
        learnRadii(of: list)

        var blockers: [CGRect] = []
        var surfaces: [WindowSurface] = []
        var dockOut = false
        var dockListed = false
        // Notification banners are in front of every window.
        for (id, bounds, sweeps) in banner.surfaces(now: now, screen: screen, menuBarHeight: menuBarHeight) {
            let frame = CGRect(x: bounds.minX - screen.minX, y: screen.maxY - bounds.maxY, width: bounds.width, height: bounds.height)
            surfaces.append(WindowSurface(
                id: id,
                frame: frame,
                blockers: blockers.filter { $0.intersects(frame) },
                cover: blockers.filter { $0.intersects(frame) },
                cornerRadius: NotificationBanner.cornerRadius,
                cornerExponent: 2.5,
                isDock: false,
                sweeps: sweeps
            ))
            blockers.append(frame)
        }
        var fresh: [CGWindowID: Double] = [:]
        var panels: [CGWindowID: CFTimeInterval] = [:]
        var windows: [CGRect] = []
        var frames: [CGWindowID: CGRect] = [:]
        var checked: [CGWindowID: (bounds: CGRect, card: Bool)] = [:]
        exposing = false
        sinkingDock = nil
        handed = [:]
        fullScreenInView = fullScreenAppHidesDock(in: list)
        fullScreenShare = fullScreenShare(in: list)
        defer {
            alphas = fresh
            panelsShownAt = panels
            covers = exposing ? [] : windows
            windowFrames = frames
            cards = checked
        }

        for entry in list {
            guard let pid = entry.pid, pid != ownPID,
                  let layer = entry.layer, layer >= 0 || isWidget(pid: pid, layer: layer), layer < maxLayer,
                  var bounds = entry.bounds,
                  let alpha = entry.alpha, alpha > 0.3,
                  !ignoredOwners.contains(entry.owner ?? "")
            else { continue }
            let id = entry.id ?? 0
            fresh[id] = alpha
            if leaving[id] != nil { continue }
            // A window fading out is closing or hiding: its snow falls as the fade begins.
            let fading = alpha < 0.98 && alpha < (alphas[id] ?? 0) - 0.01

            let owner = entry.owner ?? ""
            let pictureInPicture = layer > 0 && isPictureInPicture(pid: pid)
            let shelf = owner == "Dock" && layer == dockLevel
            if shelf {
                if fullScreenInView {
                    sinkingDock = id
                    continue
                }
                // The Dock's window spans the screen the Dock is on; only its shelf along the bottom catches snow.
                if !bounds.insetBy(dx: 1, dy: 1).intersects(screen) { continue }
                dockListed = true
                guard let dock = dockShelf(now: now, window: bounds) else { continue }
                bounds = dock
                dockOut = true
            }
            // A widget's window leaves room around it for its shadow.
            if layer < 0 { bounds = bounds.insetBy(dx: 8, dy: 8) }
            // PiP's window leaves a clear margin around the video, whatever its size: 24 points at the sides, 4 at the top
            // and bottom.
            if pictureInPicture { bounds = bounds.insetBy(dx: 24, dy: 4) }
            // Mission Control's shield, its bar of Spaces, their thumbnails and labels carry no snow.
            if owner == "WindowManager" && layer > 0 {
                if isMissionControl(bounds, layer: layer) { exposing = true }
                continue
            }
            guard bounds.intersects(screen) else {
                // Gone over to another display altogether, as a window dropped there: its snow goes along.
                if !shelf, previous.contains(where: { $0.id == id }), let display, let owner = displays.owner(of: id, bounds: bounds, now: now),
                   owner != display {
                    handed[id] = owner
                }
                continue
            }
            // Screen-sized windows above the normal layer (Notification Center, Siri) are transparent overlays. Spotlight's
            // window turns into one for a moment on every keystroke, its bar still where it was, and keeps the bar meanwhile.
            if layer > 0 && fillsScreen(bounds) {
                banner.notice(id, level: layer, pid: pid)
                guard let bar = raised[id] else { continue }
                bounds = bar.frame
            } else if layer > 0 && !shelf && bounds.minY <= screen.minY && bounds.width >= screen.width * 0.9 {
                // Notch and menu bar apps (Sapphire) lay a clear canvas across the top of the screen, from over the menu
                // bar down: it hides nothing, and the windows under it still catch snow.
                continue
            } else if layer > 0 {
                raised[id] = (bounds, now)
            }

            let frame = CGRect(x: bounds.minX - screen.minX, y: screen.maxY - bounds.maxY, width: bounds.width, height: bounds.height)
            // A window lying across displays carries its snow on one of them. Where Spaces span the displays, its part
            // here still hides the snow behind it; with Spaces of their own, that part isn't shown at all.
            if !shelf, let display, let owner = displays.owner(of: id, bounds: bounds, now: now), owner != display {
                handed[id] = owner
                if displays.spanning {
                    if layer < dockLevel && !fading { windows.append(frame) }
                    blockers.append(frame)
                }
                continue
            }
            if !showsOnDisplay(id, bounds: bounds, layer: layer, shelf: shelf) { continue }
            let roomAbove = bounds.minY - screen.minY >= menuBarHeight + 6 && frame.maxY < screen.height
            let onScreen = frame.maxX > 0 && frame.minX < screen.width && frame.maxY > 0
            let fills = fillsScreen(bounds)

            if layer < dockLevel && !fading { windows.append(frame) }
            if layer == 0 && !fading && id != 0 { frames[id] = frame }
            let coverOnly = coverOnlyOwners.contains(owner) && !shelf
            if roomAbove && onScreen && !fills && !coverOnly && !fading && bounds.width >= 80 && bounds.height >= 40, id != 0,
               shelf || !isCard(id, bounds, into: &checked) {
                // Search bars like Spotlight are capsules: low and wide, with fully rounded ends.
                let capsule = !shelf && (owner.lowercased().contains("spotlight") || (bounds.height <= 90 && bounds.width >= bounds.height * 3))
                let corner = shelf ? Dock.cornerRadius(height: bounds.height)
                    : capsule ? min(bounds.height / 2, 32)
                    : layer < 0 || pictureInPicture ? Self.widgetRadius : radii[id] ?? Self.defaultRadius
                let radius = min(corner, bounds.width / 2, bounds.height / 2)
                let region = CGRect(x: frame.minX, y: frame.maxY - radius, width: frame.width, height: 40 + radius)
                if layer > 0 && !shelf { panels[id] = panelsShownAt[id] ?? now }
                surfaces.append(WindowSurface(
                    id: id,
                    frame: frame,
                    blockers: blockers.filter { $0.intersects(region) },
                    cover: blockers.filter { $0.intersects(frame) },
                    cornerRadius: radius,
                    cornerExponent: capsule ? 2 : 2.5,
                    isDock: shelf,
                    sweeps: false
                ))
            }
            blockers.append(frame)
        }
        if !dockOut { dockShownAt = nil }
        if !dockListed {
            dockHides = false
            dockStays = false
        }
        dockWasOut = dockOut
        if let band = Marquee.rect(over: list), band.intersects(screen), band.width >= 16 {
            // The rubber band lies on the desktop, behind every window, and hides nothing: it is see-through.
            let frame = CGRect(x: band.minX - screen.minX, y: screen.maxY - band.maxY, width: band.width, height: band.height)
            let region = CGRect(x: frame.minX, y: frame.maxY, width: frame.width, height: 40)
            surfaces.append(WindowSurface(
                id: Marquee.id,
                frame: frame,
                blockers: blockers.filter { $0.intersects(region) },
                cover: blockers.filter { $0.intersects(frame) },
                cornerRadius: 0,
                cornerExponent: 2.5,
                isDock: false,
                sweeps: false
            ))
        }
        return exposing ? [] : surfaces
    }

    /// Asks the window server, once for each window that has newly shown up, how round its corners are. A window it
    /// gives no rounding, as one that draws its own frame, takes the system's.
    private func learnRadii(of list: [ListedWindow]) {
        let shown = Set(list.compactMap { entry -> CGWindowID? in
            guard let layer = entry.layer, layer >= 0, layer < dockLevel else { return nil }
            return entry.id
        })
        radii = radii.filter { shown.contains($0.key) }
        let fresh = shown.subtracting(radii.keys)
        guard !fresh.isEmpty else { return }
        let found = Corners.radii(of: Array(fresh))
        for id in fresh {
            radii[id] = found[id].flatMap { $0 > 0 ? $0 : nil } ?? Self.defaultRadius
        }
    }

    /// With separate Spaces, a window reaching over from another display isn't shown here and can't hide snow.
    private func showsOnDisplay(_ id: CGWindowID, bounds: CGRect, layer: Int, shelf: Bool) -> Bool {
        guard layer == 0, !shelf, NSScreen.screensHaveSeparateSpaces, let display else { return true }
        let reachesAnother = NSScreen.screens.contains { screen in
            let frame = screen.frame
            let other = CGRect(x: frame.minX, y: primaryHeight - frame.maxY, width: frame.width, height: frame.height)
            return other != self.screen && other.intersects(bounds)
        }
        guard reachesAnother else { return true }
        if !currentSpaceRead {
            currentSpace = Spaces.current(on: display)
            currentSpaceRead = true
        }
        guard let currentSpace, let spaces = Spaces.of(id) else { return true }
        return spaces.contains(currentSpace)
    }

    private func isWidget(pid: Int32, layer: Int) -> Bool {
        guard layer < 0, layer > desktopIconLevel else { return false }
        if let known = widgetHosts[pid] { return known }
        let host = NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == "com.apple.notificationcenterui"
        widgetHosts[pid] = host
        return host
    }

    private func isPictureInPicture(pid: Int32) -> Bool {
        if let known = pipHosts[pid] { return known }
        let host = NSRunningApplication(processIdentifier: pid)?.bundleIdentifier == "com.apple.PIPAgent"
        pipHosts[pid] = host
        return host
    }

    /// Stage Manager shows the windows of other stages as small tilted cards: the list gives the box around a card,
    /// while the window keeps its real size.
    private func isCard(_ id: CGWindowID, _ bounds: CGRect, into checked: inout [CGWindowID: (bounds: CGRect, card: Bool)]) -> Bool {
        let card: Bool
        if let known = cards[id], known.bounds == bounds {
            card = known.card
        } else if let real = SkyLight.bounds(of: id) {
            card = bounds.width < real.width * 0.6 && bounds.height < real.height * 0.6
        } else {
            card = false
        }
        checked[id] = (bounds, card)
        return card
    }
}

/// An entry of the window list, read straight off Core Foundation: bridging the whole list into Swift dictionaries
/// at every poll costs more than all the rest of the poll.
private struct ListedWindow {
    let id: CGWindowID?
    let pid: Int32?
    let layer: Int?
    let bounds: CGRect?
    let alpha: Double?
    let owner: String?
    let onScreen: Bool?

    static func all(in list: CFArray?) -> [ListedWindow] {
        guard let list else { return [] }
        return (0..<CFArrayGetCount(list)).compactMap { i in
            guard let entry = CFArrayGetValueAtIndex(list, i) else { return nil }
            return ListedWindow(Unmanaged<CFDictionary>.fromOpaque(entry).takeUnretainedValue())
        }
    }

    private init(_ entry: CFDictionary) {
        let number: Int64? = Self.number(entry, kCGWindowNumber, .sInt64Type)
        id = number.flatMap { CGWindowID(exactly: $0) }
        pid = Self.number(entry, kCGWindowOwnerPID, .sInt32Type)
        layer = Self.number(entry, kCGWindowLayer, .longType)
        alpha = Self.number(entry, kCGWindowAlpha, .doubleType)
        bounds = Self.value(entry, kCGWindowBounds, CFDictionaryGetTypeID()).flatMap {
            CGRect(dictionaryRepresentation: unsafeBitCast($0, to: CFDictionary.self))
        }
        owner = Self.value(entry, kCGWindowOwnerName, CFStringGetTypeID()).map { unsafeBitCast($0, to: CFString.self) as String }
        if let flag = Self.value(entry, kCGWindowIsOnscreen, CFBooleanGetTypeID()) {
            onScreen = CFBooleanGetValue(unsafeBitCast(flag, to: CFBoolean.self))
        } else {
            let flag: Int32? = Self.number(entry, kCGWindowIsOnscreen, .sInt32Type)
            onScreen = flag == 0 ? false : flag == 1 ? true : nil
        }
    }

    private static func value(_ entry: CFDictionary, _ key: CFString, _ type: CFTypeID) -> AnyObject? {
        guard let raw = CFDictionaryGetValue(entry, Unmanaged.passUnretained(key).toOpaque()) else { return nil }
        let value = Unmanaged<AnyObject>.fromOpaque(raw).takeUnretainedValue()
        return CFGetTypeID(value) == type ? value : nil
    }

    /// Nil unless the number fits `type` exactly, as a cast from NSNumber would be.
    private static func number<T: Numeric & BitwiseCopyable>(_ entry: CFDictionary, _ key: CFString, _ type: CFNumberType) -> T? {
        guard let value = value(entry, key, CFNumberGetTypeID()) else { return nil }
        var result: T = 0
        let exact = withUnsafeMutableBytes(of: &result) { CFNumberGetValue(unsafeBitCast(value, to: CFNumber.self), type, $0.baseAddress) }
        return exact ? result : nil
    }
}

/// A Space sliding on by itself after a swipe. The window list reports it a few frames behind the screen, but the
/// system eases it to a stop like a critically damped spring, always as stiff: a few reports tell its path, and the
/// snow keeps to that path however unevenly the list reports, until the reports leave it.
private struct Glide {
    /// The spring's natural frequency, in radians a second; it settles in about half a second.
    private static let stiffness = 13.0
    /// How far ahead of the window list's last report the screen shows the Space.
    private static let lookahead = 0.042
    private static let samples = 6

    /// How far the Space has slid since the slide began, as the window list reported it at each recent poll.
    private var path: [(time: Double, offset: Double)] = []
    private var offset = 0.0
    /// Whether, after each of the last few polls, the windows moved on within a few milliseconds.
    private var movedSoon: [Bool] = []
    /// Polls in a row that found the Space where it was.
    private var stills = 0
    /// The path the Space is taken to follow.
    private var spring: Spring?
    /// Reports in a row off that path.
    private var misses = 0
    /// How far the lead has come in, from none of it to all.
    private var weight = 0.0
    private var predicted = 0.0
    /// Where the windows rest on their Spaces, as offsets of the slide: one of them is where the slide stops.
    private var rests: [Double] = []
    /// What's left of the difference between the path given up and the one taking over, eased away over a few frames.
    private var carry = 0.0

    mutating func noteMovedSoon(_ moved: Bool) {
        movedSoon = (movedSoon + [moved]).suffix(5)
    }

    /// How far ahead of the window list to draw the Space's snow at this poll; `pitch` is how far apart Spaces slide past.
    mutating func lead(now: Double, shift: CGFloat?, toRest: [Double], pitch: Double) -> CGFloat {
        var refit = false
        if let shift, shift != 0 {
            offset += Double(shift)
            // A window rests on its own Space; the slide may stop there, or a Space either way along, if that one
            // has no windows to tell.
            rests = toRest.flatMap { [offset + $0, offset + $0 - pitch, offset + $0 + pitch] }
            path = (path + [(now, offset)]).suffix(Self.samples)
            stills = 0
            if let spring {
                // A report may show the Space as it was up to a frame and a half before the poll, under load more so.
                let (early, late) = (spring.offset(at: now - 0.025), spring.offset(at: now))
                if offset < min(early, late) - 6 || offset > max(early, late) + 6 { misses += 1; refit = true } else { misses = 0 }
            } else {
                refit = true
            }
        } else {
            stills += 1
        }
        // Fingers on the trackpad again, or a Space that stopped short of where the path ends.
        if movedSoon.filter({ $0 }).count < 2 || stills >= 3 || misses >= 2 {
            spring = nil
            misses = 0
        } else if refit, let fit = fitSpring() ?? (spring == nil ? launch() : nil) {
            if let spring { carry += spring.offset(at: now + Self.lookahead) - fit.offset(at: now + Self.lookahead) }
            spring = fit
            misses = 0
        }
        carry = spring == nil ? 0 : carry * 0.6
        if let spring {
            predicted = spring.lead(at: now + Self.lookahead, from: offset, carry: carry)
            // A path gone wrong can't fling the snow far: never more than the Space covers in a few frames.
            if let velocity { predicted = min(max(predicted, -abs(velocity) * 0.08), abs(velocity) * 0.08) }
        }
        weight = min(max(weight + (spring != nil ? 1 : -1) / 6, 0), 1)
        if weight == 0 { predicted = 0 }
        // Eased at both ends, so the snow speeds up to catch the Space, and slows back to it, without a jolt.
        let share = weight * weight * (3 - 2 * weight)
        return CGFloat(share * predicted)
    }

    /// How fast the Space went over the last three frames or more, in points a second: under load the list skips
    /// a frame, then makes up for it in one step.
    private var velocity: Double? {
        guard let to = path.last, let from = path.last(where: { to.time - $0.time >= 0.045 }) else { return nil }
        return (to.offset - from.offset) / (to.time - from.time)
    }

    /// The nearest place ahead where the slide can stop: it carries the Space on by one, or back where it was.
    private var nextRest: Double? {
        guard let to = path.last, let velocity else { return nil }
        return rests.filter { ($0 - to.offset) * velocity > 0 }.min { abs($0 - to.offset) < abs($1 - to.offset) }
    }

    /// Before there are reports enough to fit, the spring that leaves the last report as fast as the Space got
    /// there, headed for the nearest place ahead where it could stop.
    private func launch() -> Spring? {
        guard let to = path.last, let velocity, abs(velocity) > 600, let target = nextRest else { return nil }
        // Through the report, with its slope there the Space's velocity.
        let a = to.offset - target
        return Spring(target: target, a: a, b: velocity + Self.stiffness * a, time: to.time, rms: 0)
    }

    /// The spring through the last few reports. Headed where some window rests, it takes only four; with no window
    /// to tell, five decide on a target of their own.
    private func fitSpring() -> Spring? {
        guard let last = path.last else { return nil }
        let recent = path.filter { last.time - $0.time < 0.2 }
        guard recent.count >= 4 else { return nil }
        let speed = abs(last.offset - recent[0].offset) / Double(recent.count - 1)
        let tolerance = 3 + 0.1 * speed
        if let target = nextRest {
            // The last four only: the ones before may still be the fingers' push.
            let latest = Array(recent.suffix(4))
            guard latest[3].time - latest[0].time < 0.12, let held = fit(latest, target: target, at: last.time), held.rms < tolerance else { return nil }
            return held
        }
        guard rests.isEmpty, recent.count >= 5, let free = fit(recent, target: nil, at: last.time), free.rms < tolerance else { return nil }
        return free
    }

    /// Least squares through the reports, with the target given or found with the rest.
    private func fit(_ reports: [(time: Double, offset: Double)], target: Double?, at time: Double) -> Spring? {
        var m = [[Double]](repeating: [0, 0, 0], count: 3)
        var v = [0.0, 0.0, 0.0]
        for (t, offset) in reports {
            let tau = t - time
            let decay = exp(-Self.stiffness * tau)
            let row = [1, decay, tau * decay]
            for i in 0..<3 {
                v[i] += row[i] * offset
                for j in 0..<3 { m[i][j] += row[i] * row[j] }
            }
        }
        let c: [Double]
        if let target {
            // The two other rows, with the target's share moved to the right.
            let d = m[1][1] * m[2][2] - m[1][2] * m[2][1]
            guard abs(d) > 1e-12 else { return nil }
            let v1 = v[1] - m[1][0] * target, v2 = v[2] - m[2][0] * target
            c = [target, (v1 * m[2][2] - m[1][2] * v2) / d, (m[1][1] * v2 - v1 * m[2][1]) / d]
        } else {
            func det(_ a: [[Double]]) -> Double {
                a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
                    + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
            }
            let d = det(m)
            guard abs(d) > 1e-12 else { return nil }
            // Cramer's rule: each coefficient from the matrix with its column swapped for the right-hand side.
            c = (0..<3).map { k in det((0..<3).map { i in (0..<3).map { j in j == k ? v[i] : m[i][j] } }) / d }
        }
        var spring = Spring(target: c[0], a: c[1], b: c[2], time: time, rms: 0)
        spring.rms = sqrt(reports.map { pow(spring.offset(at: $0.time) - $0.offset, 2) }.reduce(0, +) / Double(reports.count))
        return spring
    }

    /// offset(t) = target + (a + b·τ)·e^(−ωτ), τ from `time`.
    private struct Spring {
        let target, a, b, time: Double
        var rms: Double

        func offset(at t: Double) -> Double {
            let tau = t - time
            return target + (a + b * tau) * exp(-Glide.stiffness * tau)
        }

        /// Never past where the spring comes to rest, nor back the way it came.
        func lead(at t: Double, from offset: Double, carry: Double) -> Double {
            let remaining = target - offset
            let lead = self.offset(at: t) + carry - offset
            return lead * remaining <= 0 ? 0 : abs(lead) > abs(remaining) ? remaining : lead
        }
    }
}

enum SkyLight {
    private static let handle = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY)

    static let connection: Int32? = function("SLSMainConnectionID", as: (@convention(c) () -> Int32).self).map { $0() }

    static func function<T>(_ name: String, as type: T.Type) -> T? {
        dlsym(handle, name).map { unsafeBitCast($0, to: type) }
    }

    private typealias Bounds = @convention(c) (Int32, CGWindowID, UnsafeMutablePointer<CGRect>) -> Int32
    private static let windowBounds = function("SLSGetWindowBounds", as: Bounds.self)

    /// A window's frame on its own Space, in the window list's coordinates: unlike the list, it stays put as the Space slides.
    static func bounds(of window: CGWindowID) -> CGRect? {
        guard let connection, let windowBounds else { return nil }
        var bounds = CGRect.zero
        return windowBounds(connection, window, &bounds) == 0 ? bounds : nil
    }
}

/// How round the window server makes windows' corners, through private SkyLight calls; without them, nothing is told.
enum Corners {
    private typealias Query = @convention(c) (Int32, CFArray, Int32) -> Unmanaged<CFTypeRef>?
    private typealias Results = @convention(c) (CFTypeRef) -> Unmanaged<CFTypeRef>?
    private typealias Advance = @convention(c) (CFTypeRef) -> Bool
    private typealias Window = @convention(c) (CFTypeRef) -> UInt32
    private typealias Radii = @convention(c) (CFTypeRef) -> Unmanaged<CFArray>?

    private static let query = SkyLight.function("SLSWindowQueryWindows", as: Query.self)
    private static let results = SkyLight.function("SLSWindowQueryResultCopyWindows", as: Results.self)
    private static let advance = SkyLight.function("SLSWindowIteratorAdvance", as: Advance.self)
    private static let window = SkyLight.function("SLSWindowIteratorGetWindowID", as: Window.self)
    private static let resolved = SkyLight.function("SLSWindowIteratorGetResolvedCornerRadii", as: Radii.self)

    /// Each window's corner radius. The query, its results and each window's radii come back owned by the caller.
    static func radii(of windows: [CGWindowID]) -> [CGWindowID: CGFloat] {
        guard let connection = SkyLight.connection, let query, let results, let advance, let window, let resolved,
              let found = query(connection, windows.map { NSNumber(value: $0) } as CFArray, 0)?.takeRetainedValue(),
              let iterator = results(found)?.takeRetainedValue()
        else { return [:] }
        var radii: [CGWindowID: CGFloat] = [:]
        while advance(iterator) {
            guard let corners = resolved(iterator)?.takeRetainedValue() as? [NSNumber], let top = corners.first else { continue }
            radii[window(iterator)] = CGFloat(top.doubleValue)
        }
        return radii
    }
}

/// The rubber band a drag across the desktop's empty space draws to select icons. Finder draws it into the desktop,
/// with no window of its own, so it is followed from the mouse: from where the button went down on the desktop to the
/// pointer, while the button is held. Dragging an icon instead shows the icon in a window at the dragging level.
enum Marquee {
    /// A window number no window has: the rubber band's.
    static let id = CGWindowID.max - 1

    /// Followed only with Deep Interaction on.
    static var enabled = false {
        didSet {
            guard enabled != oldValue else { return }
            if enabled {
                monitor = NSEvent.addGlobalMonitorForEvents(matching: .leftMouseDown) { _ in
                    let point = pointer
                    anchor = onDesktop(point) ? point : nil
                }
            } else {
                if let monitor { NSEvent.removeMonitor(monitor) }
                monitor = nil
                anchor = nil
            }
        }
    }

    private static var monitor: Any?
    /// Where the button went down on the desktop, in the window list's coordinates.
    private static var anchor: CGPoint?
    private static let draggingLevel = Int(CGWindowLevelForKey(.draggingWindow))
    private static let desktopIconLevel = Int(CGWindowLevelForKey(.desktopIconWindow))
    private static let desktopLevel = Int(CGWindowLevelForKey(.desktopWindow))

    /// The pointer in the window list's coordinates.
    private static var pointer: CGPoint {
        let location = NSEvent.mouseLocation
        return CGPoint(x: location.x, y: (NSScreen.screens.first?.frame.height ?? 0) - location.y)
    }

    /// The band, in the window list's coordinates, while one is being dragged out.
    fileprivate static func rect(over list: [ListedWindow]) -> CGRect? {
        guard enabled, let anchor else { return nil }
        guard NSEvent.pressedMouseButtons & 1 != 0 else {
            self.anchor = nil
            return nil
        }
        let point = pointer
        guard abs(point.x - anchor.x) > 3 || abs(point.y - anchor.y) > 3,
              !list.contains(where: { $0.layer == draggingLevel })
        else { return nil }
        return CGRect(x: min(anchor.x, point.x), y: min(anchor.y, point.y), width: abs(point.x - anchor.x), height: abs(point.y - anchor.y))
    }

    /// Whether the first thing drawn at this point, front to back, is the desktop: Finder's icons or the wallpaper. The
    /// window server's own clear strips over the desktop, as along the top of the screen, don't count.
    private static func onDesktop(_ point: CGPoint) -> Bool {
        let own = ProcessInfo.processInfo.processIdentifier
        let windows = ListedWindow.all(in: CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID))
        guard let hit = windows.first(where: { window in
            window.pid != own && window.owner != "Window Server" && (window.alpha ?? 0) > 0 && window.bounds?.contains(point) == true
        }), let layer = hit.layer else { return false }
        return (layer == desktopIconLevel && hit.owner == "Finder") || layer <= desktopLevel
    }
}

/// Which Spaces windows are on, through private SkyLight calls; when they're unavailable, nothing can be told.
enum Spaces {
    private typealias SpacesForWindows = @convention(c) (Int32, Int32, CFArray) -> Unmanaged<CFArray>?
    private typealias DisplaySpaces = @convention(c) (Int32) -> Unmanaged<CFArray>?

    private static let spacesForWindows = SkyLight.function("SLSCopySpacesForWindows", as: SpacesForWindows.self)
    private static let displaySpaces = SkyLight.function("SLSCopyManagedDisplaySpaces", as: DisplaySpaces.self)

    /// The Space showing on each display.
    static func current() -> Set<UInt64>? {
        guard let connection = SkyLight.connection,
              let displays = displaySpaces?(connection)?.takeRetainedValue() as? [[String: Any]]
        else { return nil }
        let spaces = displays.compactMap { ($0["Current Space"] as? [String: Any]).flatMap(id) }
        return spaces.isEmpty ? nil : Set(spaces)
    }

    /// The Space showing on each display, by the display's UUID.
    static func currentByDisplay() -> [String: UInt64]? {
        guard let connection = SkyLight.connection,
              let displays = displaySpaces?(connection)?.takeRetainedValue() as? [[String: Any]]
        else { return nil }
        var spaces: [String: UInt64] = [:]
        for display in displays {
            if let uuid = display["Display Identifier"] as? String, let current = (display["Current Space"] as? [String: Any]).flatMap(id) {
                spaces[uuid] = current
            }
        }
        return spaces.isEmpty ? nil : spaces
    }

    /// The Spaces of apps in full screen, on every display.
    static func fullScreen() -> Set<UInt64>? {
        guard let connection = SkyLight.connection,
              let displays = displaySpaces?(connection)?.takeRetainedValue() as? [[String: Any]]
        else { return nil }
        // Type 4 is a full-screen app's Space; a desktop is type 0.
        let spaces = displays.flatMap { $0["Spaces"] as? [[String: Any]] ?? [] }
        return Set(spaces.filter { ($0["type"] as? NSNumber)?.intValue == 4 }.compactMap(id))
    }

    /// The Space showing on one display.
    static func current(on display: CGDirectDisplayID) -> UInt64? {
        guard let connection = SkyLight.connection,
              let displays = displaySpaces?(connection)?.takeRetainedValue() as? [[String: Any]]
        else { return nil }
        let uuid = CGDisplayCreateUUIDFromDisplayID(display).map { CFUUIDCreateString(nil, $0.takeRetainedValue()) as String }
        guard let entry = displays.first(where: { $0["Display Identifier"] as? String == uuid }) ?? (displays.count == 1 ? displays.first : nil)
        else { return nil }
        return (entry["Current Space"] as? [String: Any]).flatMap(id)
    }

    /// The Spaces of apps in full screen on a display, and whether one of them is showing.
    static func fullScreen(on display: CGDirectDisplayID) -> (spaces: Set<UInt64>, showing: Bool)? {
        guard let connection = SkyLight.connection,
              let displays = displaySpaces?(connection)?.takeRetainedValue() as? [[String: Any]]
        else { return nil }
        let uuid = CGDisplayCreateUUIDFromDisplayID(display).map { CFUUIDCreateString(nil, $0.takeRetainedValue()) as String }
        // Without separate Spaces for each display there is one entry, "Main", for them all.
        guard let entry = displays.first(where: { $0["Display Identifier"] as? String == uuid }) ?? (displays.count == 1 ? displays.first : nil)
        else { return nil }
        let spaces = (entry["Spaces"] as? [[String: Any]] ?? []).filter { ($0["type"] as? NSNumber)?.intValue == 4 }.compactMap(id)
        let current = entry["Current Space"] as? [String: Any]
        return (Set(spaces), (current?["type"] as? NSNumber)?.intValue == 4)
    }

    static func of(_ window: CGWindowID) -> Set<UInt64>? {
        // 7 asks for the window's Spaces of every kind.
        guard let connection = SkyLight.connection,
              let spaces = spacesForWindows?(connection, 7, [NSNumber(value: window)] as CFArray)?.takeRetainedValue() as? [NSNumber],
              !spaces.isEmpty
        else { return nil }
        return Set(spaces.map(\.uint64Value))
    }

    private static func id(_ space: [String: Any]) -> UInt64? {
        (space["ManagedSpaceID"] as? NSNumber)?.uint64Value ?? (space["id64"] as? NSNumber)?.uint64Value
    }
}

/// The Dock's shelf. The window list only has a screen-sized window for the Dock, so its place comes from the
/// window server through a private SkyLight call; should that call ever go away, the Dock simply stays bare.
private enum Dock {
    private typealias GetRect = @convention(c) (Int32, UnsafeMutablePointer<CGRect>, UnsafeMutablePointer<Int32>) -> Int32

    static let revealTime: CFTimeInterval = 0.2
    /// How far above its top an auto-hidden Dock keeps out for the pointer, measured on macOS 27.
    static let holdMargin: CGFloat = 5

    private static let getRect = SkyLight.function("SLSGetDockRectWithReason", as: GetRect.self)

    /// The room the Dock keeps along the screen edge, in the window list's coordinates.
    /// An auto-hidden Dock keeps its span but no height, even while it is out.
    static var reserved: CGRect? {
        guard let connection = SkyLight.connection, let getRect else { return nil }
        var rect = CGRect.zero
        var reason: Int32 = 0
        guard getRect(connection, &rect, &reason) == 0, rect.width > 0 else { return nil }
        return rect
    }

    /// The room the Dock keeps along the bottom of `screen`, nil while it keeps none there. As the Dock moves to
    /// another display its window moves at once, but the reserved room stays on the previous display for a while:
    /// once the window covers this display alone, the Dock keeps the same room here, centred.
    static func room(on screen: CGRect, window: CGRect) -> CGRect? {
        guard let reserved else { return nil }
        if abs(reserved.maxY - screen.maxY) < 2 && reserved.midX >= screen.minX && reserved.midX <= screen.maxX {
            return reserved
        }
        guard reserved.width > reserved.height, screen.insetBy(dx: -2, dy: -2).contains(window) else { return nil }
        let width = min(reserved.width, screen.width)
        return CGRect(x: screen.midX - width / 2, y: screen.maxY - reserved.height, width: width, height: reserved.height)
    }

    private static let preferences = UserDefaults(suiteName: "com.apple.dock")

    /// Whether the Dock is set to hide by itself. The setting reaches here within a frame or two of being changed.
    static var autoHides: Bool {
        preferences?.bool(forKey: "autohide") ?? false
    }

    /// The room the Dock last kept and its icon size then.
    private static var kept: (tile: Double, height: CGFloat)?

    private static var tile: Double {
        preferences?.double(forKey: "tilesize") ?? 0
    }

    /// The room a Dock of the current icon size keeps when it stays out: as it last kept, or else reckoned from the
    /// icon size, which comes out up to a point off.
    static var height: CGFloat {
        let tile = Self.tile
        if let kept, kept.tile == tile { return kept.height }
        return (tile > 0 ? tile : 48) * 86 / 60
    }

    static func keep(_ height: CGFloat) {
        kept = (tile, height)
    }

    /// The visible shelf within the room the Dock keeps: it floats a few points above the screen edge.
    static func shelf(in room: CGRect) -> CGRect {
        CGRect(x: room.minX, y: room.minY + 1, width: room.width, height: room.height - 7)
    }

    static func cornerRadius(height: CGFloat) -> CGFloat {
        height * 0.42
    }
}
