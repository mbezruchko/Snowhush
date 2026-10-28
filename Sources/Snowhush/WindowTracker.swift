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
}

struct WindowSurface: Equatable {
    let id: CGWindowID
    /// Overlay coordinates: origin at the bottom-left of the primary screen.
    let frame: CGRect
    /// Frames of windows in front of this one, in overlay coordinates.
    let blockers: [CGRect]
    /// Top corners as a superellipse quadrant: macOS windows are close to exponent 2.5, Spotlight's capsule is a circle.
    let cornerRadius: CGFloat
    let cornerExponent: CGFloat
    /// The Dock's shelf: its snow is drawn below the Dock, so the icons stay in front of it.
    let isDock: Bool
}

/// Polls the on-screen window list: a few times a second when nothing moves, every frame while something does.
final class WindowTracker {
    /// The display tracked, in the window list's global coordinates: origin at the top left of the primary display.
    var screen: CGRect = .zero
    var menuBarHeight: CGFloat = 0

    private let ownPID = ProcessInfo.processInfo.processIdentifier
    private let maxLayer = Int(CGWindowLevelForKey(.screenSaverWindow))
    private let dockLevel = Int(CGWindowLevelForKey(.dockWindow))
    private let ignoredOwners: Set<String> = ["Window Server"]
    /// The Dock's windows (the Dock itself, the ⌘Tab switcher) hide snow behind them but don't collect any.
    private let coverOnlyOwners: Set<String> = ["Dock"]
    private var previous: [WindowSurface] = []
    private var lastQuery: CFTimeInterval = 0
    private var lastChange: CFTimeInterval = -10
    /// When an auto-hidden Dock came out; nil while it is away.
    private var dockShownAt: CFTimeInterval?
    private var alphas: [CGWindowID: Double] = [:]
    /// Windows on their way out before the window list shows it, as during a minimize's genie animation;
    /// left out until they're gone or their time is up.
    private var leaving: [CGWindowID: CFTimeInterval] = [:]
    private var pressedMinimize: CGWindowID?
    private var buttonDown = false
    private var slidingUntil: CFTimeInterval = -10
    /// When each floating panel carrying snow (Spotlight and the like) first showed.
    private var panelsShownAt: [CGWindowID: CFTimeInterval] = [:]

    /// Returns the windows when they changed since the last poll.
    /// `eager` polls every frame, e.g. while a mouse button is down and a drag may be starting.
    func poll(now: CFTimeInterval, eager: Bool) -> WindowUpdate? {
        let interval = now - lastQuery
        let minimizing = watchForMinimize(now: now)
        // Panels like Spotlight vanish on a keystroke; once one has stayed a while it's more likely a long-lived floating window.
        let panelOpen = panelsShownAt.values.contains { now - $0 < 20 }
        // Even while windows move, 60 looks a second are plenty: the snow follows a smoothed path of the window anyway.
        let closely = eager || now - lastChange < 0.6 || dockShownAt != nil || pointerAtBottomEdge || panelOpen || Self.windowsMayChange
        guard minimizing || interval >= (closely ? 1.0 / 62 : 0.2) else { return nil }
        lastQuery = now
        let surfaces = query(now: now)
        guard surfaces != previous else { return nil }
        if slides(from: previous, to: surfaces) { slidingUntil = now + 0.3 }
        let sliding = now < slidingUntil
        let vanished = Set(previous.map(\.id)).subtracting(surfaces.map(\.id))
        previous = surfaces
        lastChange = now
        let away = offscreen(among: vanished)
        // Windows carried off the screen by a sliding Space are still listed, but their snow should wait for them too.
        let elsewhere = sliding ? away.elsewhere.union(vanished.subtracting(away.parked)) : away.elsewhere
        return WindowUpdate(surfaces: surfaces, interval: Float(min(interval, 0.25)), parked: away.parked, elsewhere: elsewhere, sliding: sliding)
    }

    /// Whether every window that moved slid sideways by the same amount, unchanged otherwise, with no button held:
    /// nobody drags several windows at once, but a Space switch carries them all.
    private func slides(from old: [WindowSurface], to new: [WindowSurface]) -> Bool {
        guard NSEvent.pressedMouseButtons == 0 else { return false }
        let before = Dictionary(old.map { ($0.id, $0.frame) }, uniquingKeysWith: { first, _ in first })
        var shift: CGFloat?
        for surface in new {
            guard let frame = before[surface.id], frame != surface.frame else { continue }
            let dx = surface.frame.minX - frame.minX
            guard surface.frame.size == frame.size, abs(surface.frame.minY - frame.minY) < 0.5, abs(dx - (shift ?? dx)) < 1 else { return false }
            shift = dx
        }
        return shift != nil
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
        // 0x2E is the M key.
        if flags.contains(.maskCommand), CGEventSource.keyState(.combinedSessionState, key: 0x2E),
           let app = NSWorkspace.shared.frontmostApplication?.processIdentifier {
            let windows = Self.windowList(now: now).filter {
                $0[kCGWindowOwnerPID as String] as? Int32 == app && $0[kCGWindowLayer as String] as? Int == 0
            }
            // ⌥⌘M minimizes all of the app's windows.
            for entry in flags.contains(.maskAlternate) ? windows : Array(windows.prefix(1)) {
                guard let id = entry[kCGWindowNumber as String] as? CGWindowID, leaving[id] == nil else { continue }
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
            guard let layer = entry[kCGWindowLayer as String] as? Int, layer >= 0, layer < maxLayer,
                  entry[kCGWindowOwnerPID as String] as? Int32 != ownPID,
                  (entry[kCGWindowAlpha as String] as? Double ?? 0) > 0.3,
                  let info = entry[kCGWindowBounds as String] as? NSDictionary,
                  let bounds = CGRect(dictionaryRepresentation: info)
            else { return false }
            return bounds.contains(point)
        }
        guard let front, let id = front[kCGWindowNumber as String] as? CGWindowID, previous.contains(where: { $0.id == id }),
              let info = front[kCGWindowBounds as String] as? NSDictionary,
              let bounds = CGRect(dictionaryRepresentation: info)
        else { return nil }
        let dx = point.x - bounds.minX
        let dy = point.y - bounds.minY
        return (9...33).contains(dy) && abs(dx - dy - 23) <= 7 ? id : nil
    }

    /// A click just ended or a modifier key changed: a window may be closing (its close button, ⌘W),
    /// minimizing or hiding, and its snow should fall the moment it does.
    private static var windowsMayChange: Bool {
        CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: .leftMouseUp) < 0.5
            || CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: .flagsChanged) < 0.5
    }

    /// Polls every frame for a moment, for changes the mouse doesn't announce (app switching, Spaces).
    func watchClosely(now: CFTimeInterval) {
        lastChange = now
    }

    func reset() {
        previous = []
    }

    /// Sorts windows that left the list: closed ones and ones hidden on the current Space are in neither set.
    private func offscreen(among ids: Set<CGWindowID>) -> (parked: Set<CGWindowID>, elsewhere: Set<CGWindowID>) {
        guard !ids.isEmpty else { return ([], []) }
        // CGWindowListCreateDescriptionFromArray takes window numbers stored as raw pointer values.
        var pointers: [UnsafeRawPointer?] = ids.filter { $0 != 0 }.map { UnsafeRawPointer(bitPattern: UInt($0)) }
        guard let array = CFArrayCreate(nil, &pointers, pointers.count, nil),
              let info = CGWindowListCreateDescriptionFromArray(array) as? [[String: Any]]
        else { return ([], []) }
        let away = info.compactMap { entry in
            entry[kCGWindowIsOnscreen as String] as? Bool == true ? nil : entry[kCGWindowNumber as String] as? CGWindowID
        }
        var parked = Set<CGWindowID>()
        var elsewhere = Set<CGWindowID>()
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

    /// Where the pointer reveals an auto-hidden Dock.
    private var pointerAtBottomEdge: Bool {
        pointer.y >= screen.maxY - 3 && pointer.x >= screen.minX && pointer.x < screen.maxX
    }

    /// The pointer in the window list's coordinates.
    private var pointer: CGPoint {
        let location = NSEvent.mouseLocation
        return CGPoint(x: location.x, y: (NSScreen.screens.first?.frame.height ?? 0) - location.y)
    }

    /// The Dock's shelf while it is out, nil while it is away or leaving.
    private func dockShelf(now: CFTimeInterval) -> CGRect? {
        guard let reserved = Dock.reserved, abs(reserved.maxY - screen.maxY) < 2 else {
            dockShownAt = nil
            return nil
        }
        if reserved.height > 0 {
            dockShownAt = nil
            return Dock.shelf(in: reserved)
        }
        // Auto-hidden: it stays out only while the pointer is on it, and leaves the moment the pointer does.
        let shelf = Dock.shelf(in: CGRect(x: reserved.minX, y: screen.maxY - Dock.height, width: reserved.width, height: Dock.height))
        let pointer = pointer
        guard pointer.y >= shelf.minY, pointer.x >= reserved.minX - 8, pointer.x <= reserved.maxX + 8 else {
            dockShownAt = nil
            return nil
        }
        let shownAt = dockShownAt ?? now
        dockShownAt = shownAt
        let progress = min(1, (now - shownAt) / Dock.revealTime)
        let rise = 1 - pow(1 - progress, 3)
        return shelf.offsetBy(dx: 0, dy: (screen.maxY - shelf.minY) * (1 - rise))
    }

    private static var shared: (time: CFTimeInterval, list: [[String: Any]])?

    /// One window list serves every display's tracker that polls within the same frame.
    private static func windowList(now: CFTimeInterval) -> [[String: Any]] {
        if let shared, abs(now - shared.time) < 0.008 { return shared.list }
        let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
        shared = (now, list)
        return list
    }

    private func query(now: CFTimeInterval) -> [WindowSurface] {
        guard screen.width > 0 else { return [] }
        let list = Self.windowList(now: now)

        let visibleArea = screen.width * (screen.height - menuBarHeight)
        var blockers: [CGRect] = []
        var surfaces: [WindowSurface] = []
        var dockOut = false
        var fresh: [CGWindowID: Double] = [:]
        var panels: [CGWindowID: CFTimeInterval] = [:]
        defer {
            alphas = fresh
            panelsShownAt = panels
        }

        for entry in list {
            guard let pid = entry[kCGWindowOwnerPID as String] as? Int32, pid != ownPID,
                  let layer = entry[kCGWindowLayer as String] as? Int, layer >= 0, layer < maxLayer,
                  let boundsInfo = entry[kCGWindowBounds as String] as? NSDictionary,
                  var bounds = CGRect(dictionaryRepresentation: boundsInfo),
                  let alpha = entry[kCGWindowAlpha as String] as? Double, alpha > 0.3,
                  !ignoredOwners.contains(entry[kCGWindowOwnerName as String] as? String ?? "")
            else { continue }
            let id = entry[kCGWindowNumber as String] as? CGWindowID ?? 0
            fresh[id] = alpha
            if leaving[id] != nil { continue }
            // A window fading out is closing or hiding: its snow falls as the fade begins.
            let fading = alpha < 0.98 && alpha < (alphas[id] ?? 0) - 0.01

            let owner = entry[kCGWindowOwnerName as String] as? String ?? ""
            let shelf = owner == "Dock" && layer == dockLevel
            if shelf {
                // The Dock's window spans the screen; only its shelf along the bottom catches snow.
                guard let dock = dockShelf(now: now) else { continue }
                bounds = dock
                dockOut = true
            }
            guard bounds.intersects(screen) else { continue }
            // Screen-sized windows above the normal layer (Notification Center, Siri) are transparent overlays.
            if layer > 0 && bounds.width * bounds.height >= screen.width * screen.height * 0.9 { continue }

            let frame = CGRect(x: bounds.minX - screen.minX, y: screen.maxY - bounds.maxY, width: bounds.width, height: bounds.height)
            let roomAbove = bounds.minY - screen.minY >= menuBarHeight + 6 && frame.maxY < screen.height
            let onScreen = frame.maxX > 0 && frame.minX < screen.width && frame.maxY > 0
            let fillsScreen = bounds.width * bounds.height >= visibleArea * 0.9

            let coverOnly = coverOnlyOwners.contains(owner) && !shelf
            if roomAbove && onScreen && !fillsScreen && !coverOnly && !fading && bounds.width >= 80 && bounds.height >= 40, id != 0 {
                // Search bars like Spotlight are capsules: low and wide, with fully rounded ends.
                let capsule = !shelf && (owner.lowercased().contains("spotlight") || (bounds.height <= 90 && bounds.width >= bounds.height * 3))
                let corner = shelf ? Dock.cornerRadius(height: bounds.height) : capsule ? min(bounds.height / 2, 32) : 18
                let radius = min(corner, bounds.width / 2, bounds.height / 2)
                let region = CGRect(x: frame.minX, y: frame.maxY - radius, width: frame.width, height: 40 + radius)
                if layer > 0 && !shelf { panels[id] = panelsShownAt[id] ?? now }
                surfaces.append(WindowSurface(
                    id: id,
                    frame: frame,
                    blockers: blockers.filter { $0.intersects(region) },
                    cornerRadius: radius,
                    cornerExponent: capsule ? 2 : 2.5,
                    isDock: shelf
                ))
            }
            blockers.append(frame)
        }
        if !dockOut { dockShownAt = nil }
        return surfaces
    }
}

private enum SkyLight {
    private static let handle = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY)

    static let connection: Int32? = function("SLSMainConnectionID", as: (@convention(c) () -> Int32).self).map { $0() }

    static func function<T>(_ name: String, as type: T.Type) -> T? {
        dlsym(handle, name).map { unsafeBitCast($0, to: type) }
    }
}

/// Which Spaces windows are on, through private SkyLight calls; when they're unavailable, nothing can be told.
private enum Spaces {
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

    /// The room a Dock of the current icon size keeps when it stays out.
    static var height: CGFloat {
        let tile = UserDefaults(suiteName: "com.apple.dock")?.double(forKey: "tilesize") ?? 0
        return (tile > 0 ? tile : 48) * 86 / 60
    }

    /// The visible shelf within the room the Dock keeps: it floats a few points above the screen edge.
    static func shelf(in room: CGRect) -> CGRect {
        CGRect(x: room.minX, y: room.minY + 1, width: room.width, height: room.height - 7)
    }

    static func cornerRadius(height: CGFloat) -> CGFloat {
        height * 0.42
    }
}
