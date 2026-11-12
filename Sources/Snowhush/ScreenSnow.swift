import AppKit
import MetalKit

/// The snow on one display: its overlay window, simulation and renderer, the windows tracked on it and its light.
final class ScreenSnow {
    let displayID: CGDirectDisplayID
    let window: OverlayWindow
    let dock: DockOverlay
    let simulation: SnowSimulation
    private let view: MTKView
    private let renderer: Renderer
    private let tracker: WindowTracker
    private let displays: Displays
    private let light = SceneLight()
    private(set) var screen: NSScreen
    /// Awake and showing; at rest with nothing to draw, the view may still be paused.
    private var active = true
    /// An app in full screen on this display: the snow fades out of its way and waits until it goes.
    private(set) var covered = false
    /// A full-screen app's Space is the current one here, though Mission Control may still show over it.
    private var fullScreenCurrent = false
    /// The snow stays over full-screen apps instead of fading out of their way.
    var overFullScreen = false {
        didSet {
            guard overFullScreen != oldValue else { return }
            following = false
            arriving = false
            goneFullScreenAt = -10
            fullScreenSpaces = nil
            checkCovered()
            activityChanged()
        }
    }
    /// Mission Control shows over a full-screen app's Space, picked in it or opened from it: the snow stays until
    /// the app fills the screen.
    private var exposed = false
    private var exposedSeenAt: CFTimeInterval = 0
    /// Mission Control can drop out of the window list for a moment as it moves to a full-screen app's Space: the
    /// app covers the snow only once Mission Control has stayed away a little while.
    private var coverAfter: CFTimeInterval = 0
    /// Swiped away from the full-screen app, its Space still current while the desktop slides back in.
    private var leaving = false
    /// An app here going full screen, its Space about to slide in.
    private var arriving = false
    /// The full-screen apps' Spaces on this display at the last look.
    private var fullScreenSpaces: Set<UInt64>?
    private var spacesSeenAt: CFTimeInterval = 0
    /// When an app here last went full screen: its Space is added as the move starts, and is current only once it's in.
    private var goneFullScreenAt: CFTimeInterval = -10
    /// How much of the screen a full-screen app's Space covers as it slides, as seen without the windows tracked.
    private var untrackedShare: CGFloat = 0
    private var shareSeenAt: CFTimeInterval = 0
    /// Hidden by the window server, as while an app goes full screen.
    private var occluded = false
    /// What the overlays were last faded to; nil while they follow a slide.
    private var alphaTarget: CGFloat? = 1
    /// Following a slide between a desktop and a full-screen app: how much of the snow shows, and how much it's
    /// heading for.
    private var following = false
    private var slideAlpha: CGFloat = 1
    private var slideGoal: CGFloat = 1
    private var slideFrameAt: CFTimeInterval = 0
    /// Looks for the desktop sliding back in while a full-screen app hides the snow.
    private var lookout: Timer?
    private var asleep = false
    private var closedCheckedAt: CFTimeInterval = 0
    /// Asks for the snow to be woken or put to rest again.
    var activityChanged: () -> Void = {}
    /// Faded all the way out for a full-screen app: the snow keeps falling until then.
    private var gone = false
    private var fadeEndsAt: CFTimeInterval = 0
    /// Hidden for a full-screen app: nothing to draw.
    var givesWay: Bool { covered && !leaving && gone }
    /// Fading in or out: still drawn, though the window server may already count the overlay as hidden, as while a
    /// full-screen app picked in Mission Control zooms in.
    var fading: Bool { CACurrentMediaTime() < fadeEndsAt }
    /// The quickest the snow comes in and goes, in seconds from none to all of it.
    private static let fadeIn: CGFloat = 0.35
    private static let fadeOut: CGFloat = 0.15
    /// Picked in Mission Control or switched to, a full-screen app zooms in from where it was: the snow goes with it.
    private static let giveWay: CGFloat = 0.4
    var halloween = false {
        didSet {
            light.halloween = halloween
            light.invalidate()
            simulation.halloween = halloween
        }
    }
    var modes: [Mode] {
        get { simulation.modes }
        set { simulation.modes = newValue }
    }

    init?(screen: NSScreen, device: MTLDevice, displays: Displays, surfaces: @escaping () -> Surfaces) {
        guard let displayID = screen.displayID else { return nil }
        let simulation = SnowSimulation(
            width: Float(screen.frame.width),
            height: Float(screen.frame.height),
            intensity: Float(Preferences.intensity)
        )
        simulation.thaws = Preferences.meltSnow
        let settled = Preferences.surfaces
        simulation.accumulates = !settled.isEmpty
        simulation.groundSettles = settled.contains(.ground)
        simulation.fallingOpacity = Float(Preferences.fallingOpacity)
        let renderer: Renderer
        do {
            renderer = try Renderer(device: device, pixelFormat: .bgra8Unorm, simulation: simulation)
        } catch {
            NSLog("Snowhush: Metal setup failed: \(error)")
            return nil
        }
        self.displayID = displayID
        self.displays = displays
        tracker = WindowTracker(displays: displays)
        self.screen = screen
        self.simulation = simulation
        self.renderer = renderer
        window = OverlayWindow(screen: screen)
        window.behindWindows = Preferences.behindWindows
        window.showsInScreenshots = Preferences.showInScreenshots
        overFullScreen = Preferences.showOverFullScreen
        dock = DockOverlay(screen: screen, device: device)
        dock.window.showsInScreenshots = Preferences.showInScreenshots
        renderer.dockOverlay = dock
        view = Self.makeView(device: device, size: screen.frame.size)

        renderer.cursorLocation = { [unowned self] in
            // Notification Center is in front of the snow then, which the pointer over it can't reach.
            if window.notificationLevel != nil && tracker.notificationCoversPointer { return SIMD2(-10_000, -10_000) }
            let mouse = NSEvent.mouseLocation
            return SIMD2(Float(mouse.x - window.frame.minX), Float(mouse.y - window.frame.minY))
        }
        renderer.windowSurfaces = { [unowned self] now in
            // Behind the windows the snow still settles on them: what lies on a window's top edge is above it, in
            // plain view.
            let settled = surfaces()
            let tracked = !settled.isDisjoint(with: .tracked)
            tracker.collects = settled.intersection(.tracked)
            let update = tracked ? tracker.poll(now: now, eager: NSEvent.pressedMouseButtons != 0) : nil
            window.notificationLevel = tracked ? tracker.notificationLevel : nil
            watchForFullScreen(now: now, tracked: tracked)
            guard tracked else { return nil }
            forgetClosed(now: now)
            for (id, display) in update?.handed ?? [:] { displays.hand(id, from: self, to: display) }
            simulation.windowFrames = tracker.covers.map { SIMD4(Float($0.minX), Float($0.minY), Float($0.maxX), Float($0.maxY)) }
            return update
        }
        renderer.borrowedSnow = { [unowned self] in
            surfaces().contains(.windows) ? displays.borrowed(by: self) : []
        }
        renderer.stepped = { [unowned self] in displays.pass(from: self) }
        renderer.sceneLight = { [unowned self] now in
            light.update(screen: self.screen, now: now)
            return (light.light, light.shadowTint, light.rim, light.lantern)
        }
        renderer.rest = { [unowned self] in
            view.isPaused = true
            renderer.releaseFrames(of: view)
        }
        simulation.stirred = { [unowned self] in wake() }
        renderer.interactiveFrameRate = Self.interactiveFrameRate(of: screen)
        measure()
        view.delegate = renderer
        window.contentView = view
        window.orderFrontRegardless()
        checkCovered()
    }

    func fit(to screen: NSScreen) {
        self.screen = screen
        window.setFrame(screen.frame, display: true)
        dock.fit(to: screen)
        Renderer.fitDrawable(of: view)
        simulation.resize(width: Float(screen.frame.width), height: Float(screen.frame.height))
        light.invalidate()
        renderer.interactiveFrameRate = Self.interactiveFrameRate(of: screen)
        measure()
        checkCovered()
    }

    private static func interactiveFrameRate(of screen: NSScreen) -> Int {
        min(max(screen.maximumFramesPerSecond, 60), 120)
    }

    var maxFrameRate: Bool {
        get { renderer.maxFrameRate }
        set { renderer.maxFrameRate = newValue }
    }

    func apply(_ budget: PowerBudget) {
        renderer.budget = budget
        simulation.density = budget == .saver ? 0.75 : 1
    }

    /// `asleep` with the screens off, locked or switched away from. Inactive, nothing is seen, so the frames are given
    /// back.
    func setActive(_ active: Bool, asleep: Bool) {
        if asleep != self.asleep {
            self.asleep = asleep
            restartLookout()
        }
        self.active = active
        if !active { renderer.releaseFrames(of: view) }
        guard view.isPaused == active else { return }
        if active { Renderer.fitDrawable(of: view) }
        view.isPaused = !active
        if active { renderer.resetClock() }
    }

    private func wake() {
        guard active, view.isPaused else { return }
        Renderer.fitDrawable(of: view)
        view.isPaused = false
        renderer.resetClock()
    }

    /// Now and then the snow kept for windows away lets go of those that closed there.
    private func forgetClosed(now: CFTimeInterval) {
        guard now - closedCheckedAt >= 5 else { return }
        closedCheckedAt = now
        let away = simulation.awayIDs
        guard !away.isEmpty else { return }
        simulation.forgetClosed(keeping: WindowTracker.open(among: away))
    }

    func dropWindowSnow() {
        simulation.updateWindows([], interval: 0.1)
        simulation.windowFrames = []
        tracker.reset()
    }

    /// Going full screen, an app adds its Space as the move starts, though it is the current one only once the app is
    /// in. Swiped between a desktop and a full-screen app, the snow follows the desktop: as much of it shows as the
    /// square of the desktop's share of the screen, so it is faint while the app fills most of it.
    private func watchForFullScreen(now: CFTimeInterval, tracked: Bool) {
        guard !overFullScreen else { return }
        if covered {
            // Still drawn while fading out under the app: the lookout watches for the desktop until it comes back.
            guard leaving else { return }
            if now - shareSeenAt >= 1.0 / 30 {
                shareSeenAt = now
                guard let shown = tracker.desktopShare(now: now), shown > 0 else {
                    setLeaving(false)
                    return
                }
                slideGoal = shown * shown
            }
            follow(now: now)
            return
        }
        if now - spacesSeenAt >= 0.1 {
            spacesSeenAt = now
            if let spaces = Spaces.fullScreen(on: displayID)?.spaces {
                if let known = fullScreenSpaces, !spaces.isSubset(of: known) { goneFullScreenAt = now }
                fullScreenSpaces = spaces
            }
        }
        if fullScreenCurrent && now - exposedSeenAt >= 1.0 / 30 {
            exposedSeenAt = now
            checkCovered()
            if covered {
                activityChanged()
                return
            }
        }
        let canSlide = !(fullScreenSpaces ?? []).isEmpty
        // Untracked, the window list is read only while there is a full-screen app here to swipe to.
        if !tracked && canSlide && now - shareSeenAt >= (untrackedShare > 0 ? 1.0 / 30 : 0.1) {
            shareSeenAt = now
            untrackedShare = tracker.readFullScreenShare(now: now)
        }
        let share = !canSlide ? 0 : tracked ? tracker.fullScreenShare : untrackedShare
        let entering = now - goneFullScreenAt < 1
        if entering != arriving {
            arriving = entering
            if arriving { following = false }
            fade()
        }
        guard !arriving, share > 0 || following else { return }
        if !following { startFollowing(now: now) }
        slideGoal = (1 - share) * (1 - share)
        follow(now: now)
        if share == 0 && slideAlpha >= 0.999 {
            following = false
            fade()
        }
    }

    /// The overlays sit in a Space of their own that shows over every other, full-screen apps' too.
    private func checkCovered() {
        let now = CACurrentMediaTime()
        let was = covered
        let wasFullScreen = fullScreenCurrent
        fullScreenCurrent = !overFullScreen && Spaces.fullScreen(on: displayID)?.showing ?? false
        if fullScreenCurrent && !wasFullScreen && !was { coverAfter = now + 0.2 }
        exposed = fullScreenCurrent && (now < coverAfter || tracker.missionControlShows(now: now))
        covered = fullScreenCurrent && !exposed
        if covered != was {
            fullScreenSpaces = nil
            leaving = false
            restartLookout()
        }
        if covered && !was { following = false }
        if was && !covered {
            arriving = false
            goneFullScreenAt = -10
        }
        fade()
    }

    private func setLeaving(_ next: Bool) {
        guard next != leaving else { return }
        leaving = next
        if leaving {
            startFollowing(now: CACurrentMediaTime())
            shareSeenAt = 0
        } else {
            following = false
        }
        fade()
        activityChanged()
    }

    private func startFollowing(now: CFTimeInterval) {
        following = true
        slideAlpha = window.alphaValue
        slideFrameAt = now
        fade()
    }

    private func follow(now: CFTimeInterval) {
        guard following else { return }
        let dt = CGFloat(min(now - slideFrameAt, 0.05))
        slideFrameAt = now
        // Caught late in a quick swipe, the desktop is nearly in: the snow still comes in at its own pace.
        let step = (slideGoal - slideAlpha) * min(1, dt / 0.08)
        slideAlpha += min(max(step, -dt / Self.fadeOut), dt / Self.fadeIn)
        guard !occluded, abs(window.alphaValue - slideAlpha) > 0.002 else { return }
        window.alphaValue = slideAlpha
        dock.window.alphaValue = slideAlpha
    }

    private func restartLookout() {
        lookout?.invalidate()
        lookout = covered && !asleep ? startLookout() : nil
    }

    /// Nothing is drawn under a full-screen app, so a few times a second the window list is read for the desktop
    /// coming back, to bring the snow in with it; drawn again, the snow keeps watching each frame.
    private func startLookout() -> Timer {
        let timer = Timer(timeInterval: 0.25, repeats: true) { [weak self] _ in
            guard let self, covered, !leaving, !asleep else { return }
            if tracker.missionControlShows(now: CACurrentMediaTime()) {
                checkCovered()
                activityChanged()
                return
            }
            if let shown = tracker.desktopShare(now: CACurrentMediaTime()), shown > 0 { setLeaving(true) }
        }
        timer.tolerance = 0.05
        RunLoop.main.add(timer, forMode: .common)
        return timer
    }

    /// Gone full screen, the window server hides the overlays for the move and shows them again once the app is in:
    /// they vanish while hidden, so they come back already gone.
    private func fade() {
        if following && !occluded {
            alphaTarget = nil
            return
        }
        let out = covered || arriving || occluded
        let target: CGFloat = out ? 0 : 1
        if target != alphaTarget {
            alphaTarget = target
            gone = false
            let duration = occluded ? 0 : covered && !arriving ? Self.giveWay : out ? Self.fadeOut : 0.25
            fadeEndsAt = CACurrentMediaTime() + duration
            NSAnimationContext.runAnimationGroup { context in
                context.duration = duration
                window.animator().alphaValue = target
                dock.window.animator().alphaValue = target
            }
        }
        finishFade()
    }

    /// An animation may be lost, as on an overlay just put on screen: once the fade's time is up its end is set
    /// outright, and under a full-screen app the snow is put to rest.
    private func finishFade() {
        DispatchQueue.main.asyncAfter(deadline: .now() + max(0, fadeEndsAt - CACurrentMediaTime()) + 0.02) { [weak self] in
            guard let self, !following, let target = alphaTarget, CACurrentMediaTime() >= fadeEndsAt else { return }
            if window.alphaValue != target || dock.window.alphaValue != target {
                window.alphaValue = target
                dock.window.alphaValue = target
            }
            guard covered, !leaving, !gone, target == 0 else { return }
            gone = true
            activityChanged()
        }
    }

    func occlusionChanged() {
        let hidden = !window.occlusionState.contains(.visible)
        guard hidden != occluded else { return }
        occluded = hidden
        if hidden {
            fade()
        } else {
            checkCovered()
            if covered && !leaving, let shown = tracker.desktopShare(now: CACurrentMediaTime()), shown > 0 { setLeaving(true) }
        }
    }

    func spaceChanged() {
        checkCovered()
        simulation.spaceChanged()
        light.invalidate()
        watchWindows()
        window.holdInPlace()
        dock.window.holdInPlace()
    }

    /// Out of the screenshot tool's way, or back in place.
    func stepAside(_ aside: Bool) {
        if aside {
            window.orderOut(nil)
            dock.window.orderOut(nil)
        } else {
            window.orderFrontRegardless()
            dock.window.orderFrontRegardless()
        }
    }

    func watchWindows() {
        tracker.watchClosely(now: CACurrentMediaTime())
    }

    func close() {
        lookout?.invalidate()
        active = false
        view.isPaused = true
        view.delegate = nil
        window.orderOut(nil)
        dock.close()
    }

    /// Window bounds come in global coordinates with the origin at the top left of the primary display.
    private func measure() {
        let primaryHeight = NSScreen.screens.first?.frame.height ?? screen.frame.height
        let frame = screen.frame
        tracker.display = displayID
        tracker.screen = CGRect(x: frame.minX, y: primaryHeight - frame.maxY, width: frame.width, height: frame.height)
        tracker.primaryHeight = primaryHeight
        tracker.menuBarHeight = frame.maxY - screen.visibleFrame.maxY
        tracker.reset()
        let others = NSScreen.screens.map(\.frame).filter { $0 != frame && $0.maxY > frame.minY && $0.minY < frame.maxY }
        simulation.walls = (
            left: !others.contains { abs($0.maxX - frame.minX) < 1 },
            right: !others.contains { abs($0.minX - frame.maxX) < 1 }
        )
        simulation.openSides = (!simulation.walls.left, !simulation.walls.right)
        simulation.neighbours = displays.spanning ? simulation.openSides : (false, false)
    }

    private static func makeView(device: MTLDevice, size: CGSize) -> MTKView {
        let view = MTKView(frame: CGRect(origin: .zero, size: size), device: device)
        view.wantsLayer = true
        view.layer?.isOpaque = false
        view.colorPixelFormat = .bgra8Unorm
        view.clearColor = MTLClearColorMake(0, 0, 0, 0)
        view.framebufferOnly = true
        view.preferredFramesPerSecond = 30
        view.autoresizingMask = [.width, .height]
        view.autoResizeDrawable = false
        Renderer.fitDrawable(of: view)
        return view
    }
}

extension NSScreen {
    var displayID: CGDirectDisplayID? {
        (deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value
    }
}
