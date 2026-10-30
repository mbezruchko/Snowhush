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

    func shifted(by dx: CGFloat) -> WindowSurface {
        WindowSurface(
            id: id,
            frame: frame.offsetBy(dx: dx, dy: 0),
            blockers: blockers.map { $0.offsetBy(dx: dx, dy: 0) },
            cornerRadius: cornerRadius,
            cornerExponent: cornerExponent,
            isDock: isDock
        )
    }
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
    /// The Dock's window while a full-screen app keeps the Dock away.
    private var sinkingDock: CGWindowID?
    private var alphas: [CGWindowID: Double] = [:]
    /// Windows on their way out before the window list shows it, as during a minimize's genie animation;
    /// left out until they're gone or their time is up.
    private var leaving: [CGWindowID: CFTimeInterval] = [:]
    private var pressedMinimize: CGWindowID?
    private var buttonDown = false
    private var slidingUntil: CFTimeInterval = -10
    /// The largest window at the last poll, checked every frame until the next: a Space that starts sliding carries
    /// it off well before the next poll would see. A floating panel might stay on every Space; a window that big won't.
    private var watched: WindowSurface?
    /// A Space sliding on by itself after a swipe, for the snow to keep up with what the window list reports late.
    private var glide = Glide()
    /// How far ahead of the window list the last update drew the sliding Space's snow.
    private var lead: CGFloat = 0
    /// Windows that stay put while the Space slides, like panels on every Space: no lead for them.
    private var staying: Set<CGWindowID> = []
    /// Mission Control or App Exposé shows every window shrunk, away from where the window list puts it.
    private var exposing = false
    /// When each floating panel carrying snow (Spotlight and the like) first showed.
    private var panelsShownAt: [CGWindowID: CFTimeInterval] = [:]
    /// Every app window on the screen at the last poll, in overlay coordinates, those filling it or too tall to
    /// carry snow too.
    private(set) var covers: [CGRect] = []

    /// Returns the windows when they changed since the last poll.
    /// `eager` polls every frame, e.g. while a mouse button is down and a drag may be starting.
    func poll(now: CFTimeInterval, eager: Bool) -> WindowUpdate? {
        let interval = now - lastQuery
        let minimizing = watchForMinimize(now: now)
        // Panels like Spotlight vanish on a keystroke; once one has stayed a while it's more likely a long-lived floating window.
        let panelOpen = panelsShownAt.values.contains { now - $0 < 20 }
        // Even while windows move, 60 looks a second are plenty: the snow follows a smoothed path of the window anyway.
        let closely = eager || now - lastChange < 0.6 || dockShownAt != nil || pointerAtBottomEdge || panelOpen || exposing || Self.windowsMayChange
        // A sliding Space is followed every frame, its snow riding on the windows with no smoothing.
        guard minimizing || now < slidingUntil || interval >= (closely ? 1.0 / 62 : 0.2) || watchedMoved() else { return nil }
        lastQuery = now
        let surfaces = query(now: now)
        watched = surfaces.filter { !$0.isDock }.max { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }
        let shift = slideShift(from: previous, to: surfaces)
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
        guard surfaces != previous || ahead != lead else { return nil }
        lead = ahead
        let vanished = Set(previous.map(\.id)).subtracting(surfaces.map(\.id))
        previous = surfaces
        lastChange = now
        let away = offscreen(among: vanished)
        // Windows carried off the screen by a sliding Space or shrunk by Mission Control are still listed,
        // but their snow should wait for them too.
        var elsewhere = sliding || exposing ? away.elsewhere.union(vanished.subtracting(away.parked)) : away.elsewhere
        // The Dock stays behind as a full-screen app's Space slides in, and its snow falls.
        if let dock = sinkingDock { elsewhere.remove(dock) }
        let shown = lead == 0 ? surfaces : surfaces.map { $0.isDock || staying.contains($0.id) ? $0 : $0.shifted(by: lead) }
        let slideVelocity = shift.map { Float($0) / Float(max(interval, 1.0 / 120)) } ?? 0
        return WindowUpdate(surfaces: shown, interval: Float(min(interval, 0.25)), parked: away.parked, elsewhere: elsewhere, sliding: sliding, slideVelocity: slideVelocity)
    }

    /// How far each window carried by the sliding Space is from where it rests on its own Space: the window server keeps
    /// that frame through the slide, while the window list follows the window across the screen.
    private func distancesToRest(of surfaces: [WindowSurface]) -> [Double] {
        surfaces.filter { !$0.isDock && !staying.contains($0.id) }.prefix(4).compactMap { surface in
            SkyLight.bounds(of: surface.id).map { Double($0.minX - screen.minX - surface.frame.minX) }
        }
    }

    /// Looks at the watched window again a moment after the poll: a Space sliding on by itself moves its windows on
    /// within a few milliseconds, while one carried by fingers keeps them till the next frame.
    private func probe(_ watched: WindowSurface) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.006) { [weak self] in
            guard let self, let bounds = Self.listedBounds(of: watched.id) else { return }
            glide.noteMovedSoon(abs(bounds.minX - screen.minX - watched.frame.minX) > 0.5)
        }
    }

    /// How far every window that moved slid sideways, by the same amount and unchanged otherwise, with no button held:
    /// nobody drags several windows at once, but a Space switch carries them all. Nil when they didn't.
    private func slideShift(from old: [WindowSurface], to new: [WindowSurface]) -> CGFloat? {
        guard NSEvent.pressedMouseButtons == 0 else { return nil }
        let before = Dictionary(old.map { ($0.id, $0.frame) }, uniquingKeysWith: { first, _ in first })
        var shift: CGFloat?
        for surface in new {
            guard let frame = before[surface.id], frame != surface.frame else { continue }
            let dx = surface.frame.minX - frame.minX
            guard surface.frame.size == frame.size, abs(surface.frame.minY - frame.minY) < 0.5, abs(dx - (shift ?? dx)) < 1 else { return nil }
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
        guard let array = CFArrayCreate(nil, &pointer, 1, nil),
              let entry = (CGWindowListCreateDescriptionFromArray(array) as? [[String: Any]])?.first,
              let info = entry[kCGWindowBounds as String] as? NSDictionary
        else { return nil }
        return CGRect(dictionaryRepresentation: info)
    }

    /// Sorts windows that left the list: closed ones and ones hidden on the current Space are in neither set.
    private func offscreen(among ids: Set<CGWindowID>) -> (parked: Set<CGWindowID>, elsewhere: Set<CGWindowID>) {
        guard !ids.isEmpty else { return ([], []) }
        // CGWindowListCreateDescriptionFromArray takes window numbers stored as raw pointer values.
        var pointers: [UnsafeRawPointer?] = ids.filter { $0 != 0 }.map { UnsafeRawPointer(bitPattern: UInt($0)) }
        guard let array = CFArrayCreate(nil, &pointers, pointers.count, nil),
              let info = CGWindowListCreateDescriptionFromArray(array) as? [[String: Any]]
        else { return ([], []) }
        var away: [CGWindowID] = []
        var parked = Set<CGWindowID>()
        var elsewhere = Set<CGWindowID>()
        for entry in info {
            guard let id = entry[kCGWindowNumber as String] as? CGWindowID else { continue }
            // Gone full screen or zoomed to fill it, a window has no room for snow; its snow waits for it to come back to size.
            // Going full screen, it is off screen for the move to its own Space but already has its full-screen size.
            // Only app windows: the Dock's window spans the screen too, and its snow falls when it hides.
            if entry[kCGWindowLayer as String] as? Int == 0,
               let boundsInfo = entry[kCGWindowBounds as String] as? NSDictionary,
               let bounds = CGRect(dictionaryRepresentation: boundsInfo),
               fillsScreen(bounds) {
                elsewhere.insert(id)
            } else if entry[kCGWindowIsOnscreen as String] as? Bool != true {
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

    /// Whether an app in full screen shows on this display, its window reaching down past the Dock: the Dock sinks away
    /// as soon as the app's Space starts sliding in and stays away there, though the window server still reports it out.
    private func fullScreenAppHidesDock(in list: [[String: Any]]) -> Bool {
        let band = CGRect(x: screen.minX, y: screen.maxY - Dock.height, width: screen.width, height: Dock.height)
        let candidates = list.compactMap { entry -> CGWindowID? in
            guard entry[kCGWindowLayer as String] as? Int == 0,
                  let info = entry[kCGWindowBounds as String] as? NSDictionary,
                  let bounds = CGRect(dictionaryRepresentation: info),
                  bounds.intersects(band), fillsScreen(bounds)
            else { return nil }
            return entry[kCGWindowNumber as String] as? CGWindowID
        }
        guard !candidates.isEmpty, let fullScreen = Spaces.fullScreen(), !fullScreen.isEmpty else { return false }
        return candidates.contains { Spaces.of($0).map { !$0.isDisjoint(with: fullScreen) } ?? false }
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

        var blockers: [CGRect] = []
        var surfaces: [WindowSurface] = []
        var dockOut = false
        var fresh: [CGWindowID: Double] = [:]
        var panels: [CGWindowID: CFTimeInterval] = [:]
        var windows: [CGRect] = []
        exposing = false
        sinkingDock = nil
        defer {
            alphas = fresh
            panelsShownAt = panels
            covers = exposing ? [] : windows
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
                if fullScreenAppHidesDock(in: list) {
                    sinkingDock = id
                    continue
                }
                // The Dock's window spans the screen; only its shelf along the bottom catches snow.
                guard let dock = dockShelf(now: now) else { continue }
                bounds = dock
                dockOut = true
            }
            guard bounds.intersects(screen) else { continue }
            // Screen-sized windows above the normal layer (Notification Center, Siri) are transparent overlays.
            if layer > 0 && bounds.width * bounds.height >= screen.width * screen.height * 0.9 {
                // Mission Control and App Exposé lay a shield of their own over the screen.
                if owner == "WindowManager" { exposing = true }
                continue
            }

            let frame = CGRect(x: bounds.minX - screen.minX, y: screen.maxY - bounds.maxY, width: bounds.width, height: bounds.height)
            let roomAbove = bounds.minY - screen.minY >= menuBarHeight + 6 && frame.maxY < screen.height
            let onScreen = frame.maxX > 0 && frame.minX < screen.width && frame.maxY > 0
            let fills = fillsScreen(bounds)

            if layer < dockLevel && !fading { windows.append(frame) }
            let coverOnly = coverOnlyOwners.contains(owner) && !shelf
            if roomAbove && onScreen && !fills && !coverOnly && !fading && bounds.width >= 80 && bounds.height >= 40, id != 0 {
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
        return exposing ? [] : surfaces
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

    /// The Spaces of apps in full screen, on every display.
    static func fullScreen() -> Set<UInt64>? {
        guard let connection = SkyLight.connection,
              let displays = displaySpaces?(connection)?.takeRetainedValue() as? [[String: Any]]
        else { return nil }
        // Type 4 is a full-screen app's Space; a desktop is type 0.
        let spaces = displays.flatMap { $0["Spaces"] as? [[String: Any]] ?? [] }
        return Set(spaces.filter { ($0["type"] as? NSNumber)?.intValue == 4 }.compactMap(id))
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
