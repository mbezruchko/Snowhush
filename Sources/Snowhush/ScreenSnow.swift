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
    private let tracker = WindowTracker()
    private let light = SceneLight()
    private(set) var screen: NSScreen
    var halloween = false {
        didSet {
            light.halloween = halloween
            light.invalidate()
            simulation.halloween = halloween
        }
    }

    init?(screen: NSScreen, device: MTLDevice, snowOnWindows: @escaping () -> Bool) {
        guard let displayID = screen.displayID else { return nil }
        let simulation = SnowSimulation(
            width: Float(screen.frame.width),
            height: Float(screen.frame.height),
            snowfall: Preferences.snowfall
        )
        simulation.thaws = Preferences.meltSnow
        simulation.accumulates = Preferences.accumulate
        let renderer: Renderer
        do {
            renderer = try Renderer(device: device, pixelFormat: .bgra8Unorm, simulation: simulation)
        } catch {
            NSLog("Snowhush: Metal setup failed: \(error)")
            return nil
        }
        self.displayID = displayID
        self.screen = screen
        self.simulation = simulation
        self.renderer = renderer
        window = OverlayWindow(screen: screen)
        window.behindWindows = Preferences.behindWindows
        window.showsInScreenshots = Preferences.showInScreenshots
        dock = DockOverlay(screen: screen, device: device)
        dock.window.showsInScreenshots = Preferences.showInScreenshots
        renderer.dockOverlay = dock
        view = Self.makeView(device: device, size: screen.frame.size)

        renderer.cursorLocation = { [unowned window] in
            let mouse = NSEvent.mouseLocation
            return SIMD2(Float(mouse.x - window.frame.minX), Float(mouse.y - window.frame.minY))
        }
        renderer.windowSurfaces = { [unowned self] now in
            guard snowOnWindows() && !window.behindWindows else { return nil }
            let update = tracker.poll(now: now, eager: NSEvent.pressedMouseButtons != 0)
            simulation.windowFrames = tracker.covers.map { SIMD4(Float($0.minX), Float($0.minY), Float($0.maxX), Float($0.maxY)) }
            return update
        }
        renderer.sceneLight = { [unowned self] now in
            light.update(screen: self.screen, now: now)
            return (light.light, light.shadowTint, light.rim, light.lantern)
        }
        renderer.interactiveFrameRate = Self.interactiveFrameRate(of: screen)
        measure()
        view.delegate = renderer
        window.contentView = view
        window.orderFrontRegardless()
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
    }

    private static func interactiveFrameRate(of screen: NSScreen) -> Int {
        min(max(screen.maximumFramesPerSecond, 60), 120)
    }

    func apply(_ budget: PowerBudget) {
        renderer.budget = budget
        simulation.density = budget == .saver ? 0.75 : 1
    }

    func setActive(_ active: Bool) {
        guard view.isPaused == active else { return }
        view.isPaused = !active
        if active { renderer.resetClock() }
    }

    func dropWindowSnow() {
        simulation.updateWindows([], interval: 0.1)
        simulation.windowFrames = []
        tracker.reset()
    }

    func spaceChanged() {
        simulation.spaceChanged()
        light.invalidate()
        watchWindows()
        window.holdInPlace()
        dock.window.holdInPlace()
    }

    func watchWindows() {
        tracker.watchClosely(now: CACurrentMediaTime())
    }

    func close() {
        view.isPaused = true
        view.delegate = nil
        window.orderOut(nil)
        dock.close()
    }

    /// Window bounds come in global coordinates with the origin at the top left of the primary display.
    private func measure() {
        let primaryHeight = NSScreen.screens.first?.frame.height ?? screen.frame.height
        let frame = screen.frame
        tracker.screen = CGRect(x: frame.minX, y: primaryHeight - frame.maxY, width: frame.width, height: frame.height)
        tracker.menuBarHeight = frame.maxY - screen.visibleFrame.maxY
        tracker.reset()
        let others = NSScreen.screens.map(\.frame).filter { $0 != frame && $0.maxY > frame.minY && $0.minY < frame.maxY }
        simulation.walls = (
            left: !others.contains { abs($0.maxX - frame.minX) < 1 },
            right: !others.contains { abs($0.minX - frame.maxX) < 1 }
        )
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
