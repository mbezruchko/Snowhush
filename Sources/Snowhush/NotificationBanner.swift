import AppKit

/// Notification banners at the top right of the screen. The window list has no window for a banner, only Notification
/// Center's clear window the size of the screen: it shows as a banner starts to slide in and hides a moment after the
/// last one is gone. Where a banner is comes from the fixed way the system lays it out and moves it, and when it moves
/// from Notification Center getting busy: it uses no processor at all while its banners stay still. A banner taking the
/// place of another follows a notification coming in, which keeps usernoted, the daemon delivering it, busy for a moment.
struct NotificationBanner {
    /// Stand in for the banners' windows, one coming as another goes: no window, nor the rubber band, has these numbers.
    static let ids: [CGWindowID] = [.max - 2, .max - 3]
    static let cornerRadius: CGFloat = 18
    private static let width: CGFloat = 344
    /// A banner with a title and one line of text; longer ones reach further down from the same top.
    private static let height: CGFloat = 56
    /// A banner's distance from the right edge of the screen and from the menu bar.
    private static let margin: CGFloat = 16
    /// How far right of its place a banner starts, just off the screen.
    private static let travel = margin + width
    /// How long after Notification Center's window shows the banner starts to slide in.
    private static let delay = 0.03
    /// How long after the banner it replaces starts to leave a new banner starts to slide in.
    private static let swapDelay = 0.05
    /// The slide in eases to a stop, the distance left shrinking by a factor of e in this time.
    private static let easing = 0.064
    /// A banner left alone slides out `lifetime` after it started to slide in; one the pointer kept up, no sooner than
    /// `lingering` after the pointer left it.
    private static let lifetime = 4.97
    private static let lingering = 0.54
    /// Closed with its button, a banner shrinks away where it is this long after the click.
    private static let closing = 0.15
    /// The slide out leaves at an even pace, off the screen in this time.
    private static let exit = 0.135
    /// Notification Center animating takes about 10 ms of processor time a second; idle, it takes next to none.
    private static let busyRate = 0.002
    /// Sliding a banner out takes several times what hovering over it does, so it shows even while the hover keeps
    /// Notification Center busy: this much processor time, in seconds, over the last `surgeSpan` seconds. A moment's
    /// work, as when a banner's app opens, peaks as high but doesn't last.
    private static let surgeWork = 0.0014
    private static let surgeSpan = 0.2
    /// A frame takes this share of its time or more once a slide starts.
    private static let surgeRate = 0.006
    /// An animation keeps it busy for half a second; a moment's work, as when it files a banner away, is shorter than this.
    private static let animating = 0.05
    /// Notification Center opened as a whole fills a column this wide along the right edge of the screen.
    private static let panelWidth: CGFloat = 416
    /// usernoted takes about 0.3 ms to deliver a notification and next to none otherwise, in seconds.
    private static let delivery = 0.0001

    private struct Banner {
        let id: CGWindowID
        /// When it starts to slide in.
        let start: CFTimeInterval
        var leavingAt: CFTimeInterval?
        /// Where it was when it started to leave, as a distance from its place.
        var leftFrom: CGFloat = 0
        /// Up after its time, as an alert waiting to be closed is, or in place of one that left unseen: it stays put.
        var staying = false
        /// With motion reduced, banners fade in and out where they are instead of sliding.
        var fades = false

        /// How far right of its place the banner is; nil before it slides in and after it has left.
        func offset(now: CFTimeInterval) -> CGFloat? {
            if staying { return 0 }
            if fades { return now >= start && leavingAt.map { now < $0 } ?? true ? 0 : nil }
            if let leavingAt {
                let progress = (now - leavingAt) / NotificationBanner.exit
                return progress < 1 ? leftFrom + (NotificationBanner.travel - leftFrom) * progress : nil
            }
            let elapsed = now - start
            guard elapsed > 0 else { return nil }
            let offset = NotificationBanner.travel * exp(-elapsed / NotificationBanner.easing)
            return offset < 0.25 ? 0 : offset
        }
    }

    /// Notification Center's process, and when it was last looked for while not running.
    private var process: pid_t?
    private var processSought: CFTimeInterval = -10
    /// Notification Center's window, its level and its process, once found, and when it was last looked for in vain.
    private var window: (id: CGWindowID, level: Int, pid: pid_t)?
    private var windowSought: CFTimeInterval = -10
    private var processorTime: UInt64 = 0
    /// usernoted's process, and when it was last looked for while not running.
    private var usernoted: pid_t?
    private var usernotedSought: CFTimeInterval = -10
    private var deliveryTime: UInt64 = 0
    private var deliveredAt: CFTimeInterval = -10
    private var lastUpdate: CFTimeInterval = 0
    private var busyAt: CFTimeInterval = -10
    /// When Notification Center got busy after a quiet spell, and whether that has been taken for an animation yet.
    private var busySince: CFTimeInterval = -10
    private var animationNoted = false
    /// Notification Center's work over the last frames: when each frame began, the processor time it used, and
    /// whether that was a slide's share.
    private var recentWork: [(start: CFTimeInterval, used: Double, sliding: Bool)] = []
    /// When the work surging now began, if it does.
    private var surgeStart: CFTimeInterval?
    /// Whether Notification Center's window is up, showing banners or opened as a whole.
    private(set) var windowShown = false
    private var buttonDown = false
    /// The banner showing or sliding in, and the one sliding out.
    private var current: Banner?
    private var leaving: Banner?
    private var hoveredAt: CFTimeInterval = -10
    /// Notification Center opened as a whole, in the same window: no banners till it closes.
    private var panel = false

    /// Looks at Notification Center's processor time every frame, at a negligible cost, and at its window only while it
    /// is busy or showing. Returns whether a banner is moving, when the windows have to be polled every frame too.
    mutating func update(now: CFTimeInterval, screen: CGRect, menuBarHeight: CGFloat, pointer: CGPoint) -> Bool {
        let busy = isBusy(now: now)
        noteDelivery(now: now)
        if busy && now - busyAt > 0.15 {
            busySince = now
            animationNoted = false
        }
        if busy { busyAt = now }
        // The animation started when Notification Center got busy, a frame or two before it can be told from a moment's work.
        let animationStart: CFTimeInterval? = busy && !animationNoted && now - busySince >= Self.animating ? busySince : nil
        if animationStart != nil { animationNoted = true }
        let shown = (busy || windowShown) && isShown(on: screen)
        defer { windowShown = shown }
        let down = NSEvent.pressedMouseButtons & 1 != 0
        defer { buttonDown = down }
        if leaving.map({ $0.offset(now: now) == nil }) == true { leaving = nil }

        if shown && !windowShown {
            panel = Self.panelOpening(pointerInMenuBar: pointer.y < screen.minY + menuBarHeight)
            current = panel ? nil : Banner(id: nextID, start: now + Self.delay, fades: Self.fading)
            hoveredAt = -10
            return true
        }
        guard shown, !panel else {
            // Gone with Notification Center's window, as after a swipe, or covered by it opened as a whole: it vanishes
            // where it is.
            current = nil
            return leaving != nil
        }
        let place = place(on: screen, menuBarHeight: menuBarHeight)
        guard let banner = current else {
            // The window stayed up after the banner left: one still shows.
            if leaving == nil && now - busyAt > 0.4 { current = Banner(id: nextID, start: now, staying: true) }
            return leaving != nil
        }
        // The close button, on the banner's top left corner, dismisses it at once, and it shrinks away where it is; a
        // click elsewhere on it may open its app and leave it be.
        let closeButton = CGRect(x: place.minX - 6, y: place.minY - 6, width: 24, height: 24)
        if closeButton.contains(pointer) && buttonDown && !down {
            leaving = banner
            leaving?.staying = false
            leaving?.fades = true
            leaving?.leavingAt = now + Self.closing
            current = nil
            return true
        }
        let age = now - banner.start
        let pointed = age > 0 && place.insetBy(dx: -12, dy: -12).contains(pointer)
        if pointed { hoveredAt = now }
        // Hovering animates the banner too; once the pointer leaves, the banner goes soon after.
        let hovered = hoveredAt > banner.start
        let due = max(Self.lifetime, hovered ? hoveredAt - banner.start + Self.lingering : 0)
        let settled = age > 8 * Self.easing && now - hoveredAt > 0.3
        // After a hover Notification Center stays busy until the banner goes: no burst of its own marks the slide,
        // only a surge in the work.
        let surge = hovered && settled ? surgeStart : nil
        // Busy just after a notification came in, another banner takes this one's place; busy otherwise, this one
        // leaves, if its time is near, or Notification Center merely did some work of its own. One that stays until
        // dismissed sees plenty of such work and goes only on a surge.
        let motion = banner.staying ? settled ? surgeStart : nil : animationStart.flatMap { settled ? $0 : nil } ?? surge
        let swap = motion.map { $0 - deliveredAt < 0.4 } ?? false
        let start = motion.flatMap { swap || banner.staying || age > due - 0.5 ? $0 : nil }
        if start == nil && !banner.staying && age > due + (hovered ? 1 : 0.5) {
            // An alert stays until dismissed; a banner gone unnoticed takes its window with it soon after.
            current?.staying = true
            return false
        }
        if let start {
            leaving = banner
            leaving?.staying = false
            leaving?.leavingAt = start
            leaving?.leftFrom = banner.offset(now: start) ?? Self.travel
            current = swap ? Banner(id: nextID, start: start + Self.swapDelay, fades: banner.fades) : nil
            return true
        }
        return age < 8 * Self.easing || leaving != nil
    }

    /// A screen-sized window above the others in the window list: Notification Center's, if `pid` is its process.
    mutating func notice(_ id: CGWindowID, level: Int, pid: pid_t) {
        if window?.id != id && pid == process { window = (id, level, pid) }
    }

    /// The level of Notification Center's window while it is up.
    var level: Int? {
        windowShown ? window?.level : nil
    }

    /// The banners' frames now, in the window list's coordinates, and whether each sweeps the snow aside as it slides in.
    func surfaces(now: CFTimeInterval, screen: CGRect, menuBarHeight: CGFloat) -> [(id: CGWindowID, frame: CGRect, sweeps: Bool)] {
        let place = place(on: screen, menuBarHeight: menuBarHeight)
        return [current, leaving].compactMap { banner in
            guard let banner, let offset = banner.offset(now: now) else { return nil }
            return (banner.id, place.offsetBy(dx: offset, dy: 0), banner.leavingAt == nil && !banner.staying && !banner.fades)
        }
    }

    /// Notification Center opened as a whole, in the window list's coordinates: the pointer over it can't reach the snow
    /// behind it.
    func panelFrame(screen: CGRect, menuBarHeight: CGFloat) -> CGRect? {
        guard windowShown && panel else { return nil }
        return CGRect(x: screen.maxX - Self.panelWidth, y: screen.minY + menuBarHeight, width: Self.panelWidth, height: screen.height - menuBarHeight)
    }

    private static var fading: Bool {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    }

    /// The stand-in number the banner going out isn't using.
    private var nextID: CGWindowID {
        Self.ids.first { $0 != current?.id && $0 != leaving?.id } ?? Self.ids[0]
    }

    private func place(on screen: CGRect, menuBarHeight: CGFloat) -> CGRect {
        CGRect(x: screen.maxX - Self.margin - Self.width, y: screen.minY + menuBarHeight + Self.margin, width: Self.width, height: Self.height)
    }

    /// Notes when usernoted last delivered a notification.
    private mutating func noteDelivery(now: CFTimeInterval) {
        if usernoted == nil && now - usernotedSought > 1 {
            usernotedSought = now
            usernoted = Self.daemon(named: "usernoted")
            deliveryTime = 0
        }
        guard let pid = usernoted else { return }
        guard let time = Self.processorTime(of: pid) else {
            usernoted = nil
            return
        }
        if deliveryTime > 0 && Double(time &- deliveryTime) / 1e9 > Self.delivery { deliveredAt = now }
        deliveryTime = time
    }

    /// Whether Notification Center used the processor since the last frame at the pace of an animation.
    private mutating func isBusy(now: CFTimeInterval) -> Bool {
        surgeStart = nil
        if process == nil && now - processSought > 1 {
            processSought = now
            process = NSRunningApplication.runningApplications(withBundleIdentifier: Self.bundleID).first?.processIdentifier
            processorTime = 0
        }
        guard let pid = process else { return false }
        guard let time = Self.processorTime(of: pid) else {
            // Notification Center quit, to be started again.
            process = nil
            window = nil
            return false
        }
        let elapsed = now - lastUpdate
        defer {
            processorTime = time
            lastUpdate = now
        }
        let used = Double(time &- processorTime) / 1e9
        // Its window may come and go with the banners: it is looked for again once Notification Center gets busy.
        if window == nil && processorTime > 0 && used > Self.busyRate * min(elapsed, 0.1) && now - windowSought > 0.25 {
            window = Self.find(of: pid)
            if window == nil { windowSought = now }
        }
        if processorTime > 0 { recentWork.append((now - min(elapsed, 0.1), used, used > Self.surgeRate * min(elapsed, 0.1))) }
        recentWork.removeAll { now - $0.start > Self.surgeSpan }
        if recentWork.reduce(0, { $0 + $1.used }) > Self.surgeWork { surgeStart = recentWork.first(where: \.sliding)?.start }
        return used > Self.busyRate * min(elapsed, 0.1)
    }

    private mutating func isShown(on screen: CGRect) -> Bool {
        guard let id = window?.id else { return false }
        // CGWindowListCreateDescriptionFromArray takes window numbers stored as raw pointer values.
        var pointer = UnsafeRawPointer(bitPattern: UInt(id))
        guard let array = CFArrayCreate(nil, &pointer, 1, nil),
              let entry = (CGWindowListCreateDescriptionFromArray(array) as? [[String: Any]])?.first
        else {
            // Gone: a new one is looked for as Notification Center gets busy again.
            window = nil
            return false
        }
        guard entry[kCGWindowIsOnscreen as String] as? Bool == true,
              let info = entry[kCGWindowBounds as String] as? NSDictionary,
              let bounds = CGRect(dictionaryRepresentation: info)
        else { return false }
        return bounds.intersects(screen)
    }

    /// Notification Center's window, even while hidden, which the on-screen window list leaves out.
    private static func find(of process: pid_t) -> (id: CGWindowID, level: Int, pid: pid_t)? {
        let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
        for entry in list {
            guard let layer = entry[kCGWindowLayer as String] as? Int, layer > 0,
                  let id = entry[kCGWindowNumber as String] as? CGWindowID,
                  entry[kCGWindowOwnerPID as String] as? pid_t == process,
                  let info = entry[kCGWindowBounds as String] as? NSDictionary,
                  let bounds = CGRect(dictionaryRepresentation: info),
                  NSScreen.screens.contains(where: { $0.frame.size == bounds.size })
            else { continue }
            return (id, layer, process)
        }
        return nil
    }

    /// A process's processor time so far, in nanoseconds.
    private static func processorTime(of pid: pid_t) -> UInt64? {
        var info = rusage_info_v2()
        let read = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V2, $0) }
        }
        return read == 0 ? info.ri_user_time + info.ri_system_time : nil
    }

    /// This user's process of that name, such as one of the system's daemons.
    private static func daemon(named name: String) -> pid_t? {
        var pids = [pid_t](repeating: 0, count: Int(proc_listallpids(nil, 0)) + 64)
        let count = Int(proc_listallpids(&pids, Int32(pids.count * MemoryLayout<pid_t>.size)))
        return pids.prefix(max(count, 0)).first { pid in
            var info = proc_bsdinfo()
            guard pid > 0, proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, Int32(MemoryLayout<proc_bsdinfo>.size)) > 0, info.pbi_uid == getuid() else {
                return false
            }
            return withUnsafeBytes(of: info.pbi_comm) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) } == name
        }
    }

    private static let bundleID = "com.apple.notificationcenterui"

    /// Notification Center opens as a whole with a click on the clock in the menu bar, a swipe in from the trackpad's
    /// right edge or fn-N, in the same window as the banners.
    private static func panelOpening(pointerInMenuBar: Bool) -> Bool {
        let source = CGEventSourceStateID.combinedSessionState
        let clicked = pointerInMenuBar && CGEventSource.secondsSinceLastEventType(source, eventType: .leftMouseDown) < 0.5
        // 31 is a swipe, an event type CGEventType has no name for.
        let swiped = CGEventType(rawValue: 31).map { CGEventSource.secondsSinceLastEventType(source, eventType: $0) < 0.5 } ?? false
        return clicked || swiped || CGEventSource.flagsState(source).contains(.maskSecondaryFn)
    }
}
