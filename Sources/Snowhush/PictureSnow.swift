import Accelerate
import AppKit
import IOSurface
import Metal

/// A window at the desktop's level, just over the wallpaper and under the desktop's icons, its widgets and every other
/// window: what lies in it lies in the wallpaper. The window server hides it behind whatever is in front, lets the
/// see-through parts of other windows pick it up, and slides it along with the Space it is on.
final class PictureWindow: NSWindow {
    static let desktopLevel = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.desktopWindow)) + 1)

    /// The snow's picture; a layer of its own, so its changes fade only when asked to.
    let snow = CALayer()

    /// `everySpace` when the Spaces can't be told apart: it is then on all of them at once.
    init(screen: NSScreen, everySpace: Bool) {
        super.init(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isReleasedWhenClosed = false
        animationBehavior = .none
        collectionBehavior = everySpace ? [.canJoinAllSpaces, .stationary, .ignoresCycle] : [.stationary, .ignoresCycle]
        level = Self.desktopLevel
        sharingType = .none
        let view = NSView(frame: NSRect(origin: .zero, size: screen.frame.size))
        view.wantsLayer = true
        contentView = view
        snow.frame = view.bounds
        snow.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
        snow.contentsGravity = .resize
        snow.actions = ["contents": NSNull(), "opacity": NSNull(), "bounds": NSNull(), "position": NSNull()]
        view.layer?.addSublayer(snow)
        setFrame(screen.frame, display: false)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    // Borderless windows are otherwise pushed below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}

/// The snow lying in the wallpaper of one display. Each desktop Space has a window of its own with the snow of its
/// own wallpaper, made when the Space is first shown; it is drawn again, a second apart at most, only while the snow
/// in it changes and something of it can be seen.
final class PictureSnow {
    /// A picture of the snow and the texture that draws into it.
    private struct Surface {
        let surface: IOSurfaceRef
        let texture: MTLTexture
    }

    /// One Space's desktop.
    private final class Desktop {
        let window: PictureWindow
        var picture: Wallpaper.Picture?
        var scene: WallpaperScene?
        var front: Surface?
        var level: Float = -1
        var dabs = -1
        var drawing = false
        var drawnAt: CFTimeInterval = 0
        /// Shown at half its size while its Space is away, to take a quarter of the memory.
        var shrunk = false
        var observer: NSObjectProtocol?

        init(window: PictureWindow) {
            self.window = window
        }
    }

    private let device: MTLDevice
    private let renderer: PictureRenderer
    private unowned let simulation: SnowSimulation
    private let displayID: CGDirectDisplayID
    private var screen: NSScreen
    /// The desktops shown so far, by their Space; under 0 the one for every Space, when Spaces can't be told apart.
    private var desktops: [UInt64: Desktop] = [:]
    private weak var current: Desktop?
    /// The wallpaper the renderer has taken in.
    private var loaded: Wallpaper.Picture?
    private var timer: Timer?
    private var active = true
    /// Whether the system has been asked for the wallpaper since the Space or the screen last changed, when, and the
    /// date its store of wallpapers had then.
    private var asked = false
    private var askedAt: CFTimeInterval = 0
    private var storeStamp: Int?
    private static let store = NSHomeDirectory() + "/Library/Application Support/com.apple.wallpaper/Store/Index.plist"

    /// One wallpaper at a time is looked at, for all the displays: each look holds a few screens' worth of pixels.
    private static let lookout = DispatchQueue(label: "com.mbezruchko.Snowhush.wallpaper", qos: .utility)
    /// How much more of the snow must have fallen or melted for it to be drawn again.
    private static let step: Float = 0.004

    var showsInScreenshots = false {
        didSet { for desktop in desktops.values { desktop.window.sharingType = showsInScreenshots ? .readOnly : .none } }
    }

    /// Snow settles in the wallpaper at all.
    var wanted = false {
        didSet {
            guard wanted != oldValue else { return }
            if wanted {
                wake()
            } else {
                rest()
                for desktop in desktops.values { close(desktop) }
                desktops = [:]
                loaded = nil
                simulation.setPicture(nil)
            }
        }
    }

    init?(screen: NSScreen, device: MTLDevice, simulation: SnowSimulation) {
        guard let displayID = screen.displayID, let renderer = try? PictureRenderer(device: device) else { return nil }
        self.displayID = displayID
        self.screen = screen
        self.device = device
        self.renderer = renderer
        self.simulation = simulation
    }

    func fit(to screen: NSScreen) {
        self.screen = screen
        for desktop in desktops.values {
            desktop.window.setFrame(screen.frame, display: false)
            desktop.level = -1
        }
        asked = false
        look()
    }

    /// `active` while the display is awake and shown.
    func setActive(_ active: Bool) {
        self.active = active
        if active { wake() } else { rest() }
    }

    /// Starts looking again, a second apart: for another wallpaper, and for snow to draw.
    func wake() {
        guard wanted, active, timer == nil else { return }
        let timer = Timer(timeInterval: 1, repeats: true) { [weak self] _ in self?.look() }
        timer.tolerance = 0.3
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        look()
    }

    private func rest() {
        timer?.invalidate()
        timer = nil
    }

    func close() {
        rest()
        for desktop in desktops.values { close(desktop) }
        desktops = [:]
    }

    private func close(_ desktop: Desktop) {
        if let observer = desktop.observer { NotificationCenter.default.removeObserver(observer) }
        desktop.window.close()
    }

    /// Another Space is showing: its desktop takes over, and the others, out of sight, are kept small.
    func spaceChanged() {
        guard wanted else { return }
        for (space, desktop) in desktops where space != 0 {
            // A Space taken away hands its windows to another, which has a desktop of its own already.
            if let spaces = Spaces.of(CGWindowID(desktop.window.windowNumber)), !spaces.contains(space) {
                close(desktop)
                desktops[space] = nil
            }
        }
        asked = false
        look()
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in
            guard let self else { return }
            for desktop in desktops.values where desktop !== current { shrink(desktop) }
        }
    }

    /// The window picker of a screenshot takes any window drawn into for a solid one: the desktop showing steps out
    /// of its way, and comes back once it is done.
    func stepAside(_ aside: Bool) {
        guard let desktop = current else { return }
        if aside {
            desktop.window.orderOut(nil)
        } else if let space = desktops.first(where: { $0.value === desktop })?.key, space == 0 || Spaces.current(on: displayID) == space {
            desktop.window.orderFrontRegardless()
        } else {
            // Its Space is no longer the one showing, and ordering it front would bring it over to this one.
            close(desktop)
            desktops = desktops.filter { $0.value !== desktop }
        }
    }

    /// A gust has cleared the snow: the picture's goes with it.
    func cleared() {
        for desktop in desktops.values where desktop !== current { clear(desktop) }
        guard let desktop = current, desktop.front != nil else { return }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = 1
        fade.toValue = 0
        fade.duration = 1.2
        fade.timingFunction = CAMediaTimingFunction(name: .easeIn)
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak self, weak desktop] in
            guard let desktop else { return }
            self?.clear(desktop)
            desktop.window.snow.opacity = 1
        }
        desktop.window.snow.opacity = 0
        desktop.window.snow.add(fade, forKey: "clearing")
        CATransaction.commit()
    }

    private func clear(_ desktop: Desktop) {
        desktop.window.snow.contents = nil
        desktop.front = nil
        desktop.shrunk = false
        desktop.level = -1
    }

    /// Asking the system which wallpaper a screen shows takes milliseconds, too long to do every second. The file the
    /// system keeps its wallpapers in is rewritten whenever one changes, and looking at its date costs next to nothing:
    /// the system is asked only then, when the Space or the screen changed, and every half a minute to be safe.
    /// Without that file it is asked every five seconds.
    private func wallpaperMayHaveChanged() -> Bool {
        let now = CACurrentMediaTime()
        var info = stat()
        let stamp = stat(Self.store, &info) == 0 ? info.st_mtimespec.tv_sec * 1_000_000_000 + info.st_mtimespec.tv_nsec : nil
        let changed = !asked || stamp != storeStamp || now - askedAt >= (stamp == nil ? 5 : 30)
        guard changed else { return false }
        asked = true
        askedAt = now
        storeStamp = stamp
        return true
    }

    /// The desktop of the Space now showing, made if this is its first time; nil on a full-screen app's Space.
    private func desktopShowing() -> Desktop? {
        var space: UInt64 = 0
        if let showing = Spaces.current(on: displayID) {
            if Spaces.fullScreen(on: displayID)?.showing == true { return nil }
            space = showing
        }
        if let desktop = desktops[space] { return desktop }
        let window = PictureWindow(screen: screen, everySpace: space == 0)
        window.sharingType = showsInScreenshots ? .readOnly : .none
        // Ordered front once and never again: that puts a window on the Space showing, and would move it there later.
        window.orderFrontRegardless()
        let desktop = Desktop(window: window)
        desktop.observer = NotificationCenter.default.addObserver(forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main) { [weak self] _ in
            self?.look()
        }
        desktops[space] = desktop
        return desktop
    }

    /// Looks at the desktop showing: for another wallpaper on it, which is then read off the main thread, and for
    /// snow that has changed, which is drawn. With nothing falling, lying or melting it rests until woken.
    private func look() {
        guard wanted, active, let desktop = desktopShowing() else { return }
        if desktop !== current {
            current = desktop
            simulation.setPicture(desktop.scene?.terrain)
            asked = false
        }
        let picture = wallpaperMayHaveChanged() ? Wallpaper.picture(on: screen) : desktop.picture
        if picture != desktop.picture {
            desktop.picture = picture
            desktop.scene = nil
            fade(desktop.window.snow, over: 0.3)
            clear(desktop)
            simulation.setPicture(nil)
            if let picture {
                let columnWidth = simulation.columnWidth
                Self.lookout.async { [weak self, weak desktop] in
                    let scene = Wallpaper.scene(in: picture, columnWidth: columnWidth)
                    DispatchQueue.main.async {
                        guard let self, let desktop, desktop.picture == picture else { return }
                        desktop.scene = scene
                        if desktop === self.current { self.simulation.setPicture(scene?.terrain) }
                        self.look()
                    }
                }
            }
        }
        guard let scene = desktop.scene, !desktop.drawing else { return }
        let level = simulation.pictureLevel
        guard level > 0 || simulation.hasDabs else {
            if desktop.front != nil { clear(desktop) }
            if simulation.isEmpty { rest() }
            return
        }
        let now = CACurrentMediaTime()
        // What the flakes seen landing add in a second can't be told; in a few it can.
        let changed = abs(level - desktop.level) >= Self.step || desktop.shrunk
            || (desktop.dabs != simulation.dabsVersion && now - desktop.drawnAt >= 3)
        // Behind other windows it is drawn now and then all the same, so that uncovered it is never far behind.
        guard changed, desktop.window.occlusionState.contains(.visible) || now - desktop.drawnAt > 15 else { return }
        draw(desktop, scene: scene, level: level, now: now)
    }

    private func draw(_ desktop: Desktop, scene: WallpaperScene, level: Float, now: CFTimeInterval) {
        // Each picture is drawn into a surface of its own, and the one before let go once this one shows: drawing into
        // the one on show would flicker, and keeping a second for next time would double the memory.
        let size = screen.frame.size
        guard let target = makeSurface(width: Int(size.width), height: Int(size.height)) else { return }
        if loaded != desktop.picture || !renderer.loaded {
            renderer.load(scene, columnWidth: simulation.columnWidth)
            loaded = desktop.picture
        }
        let dabs = simulation.dabsVersion
        let first = desktop.front == nil
        desktop.drawing = true
        let drawn = renderer.draw(level: level, dabs: simulation.dabs, dabsSize: simulation.dabsSize, dabsVersion: dabs, into: target.texture,
                                  screen: SIMD2(Float(size.width), Float(size.height))) { [weak self, weak desktop] in
            guard let self, let desktop else { return }
            desktop.drawing = false
            // The first snow on a wallpaper comes in softly; after that each picture differs from the last by less
            // than the eye can tell, and just takes its place.
            if first { fade(desktop.window.snow, over: 0.6) }
            desktop.window.snow.contents = target.surface
            desktop.front = target
            desktop.shrunk = false
            desktop.level = level
            desktop.dabs = dabs
            desktop.drawnAt = now
        }
        if !drawn { desktop.drawing = false }
    }

    private func fade(_ layer: CALayer, over duration: CFTimeInterval) {
        let fade = CATransition()
        fade.type = .fade
        fade.duration = duration
        layer.add(fade, forKey: "contents")
    }

    private func makeSurface(width: Int, height: Int) -> Surface? {
        let properties: [CFString: Any] = [
            kIOSurfaceWidth: width,
            kIOSurfaceHeight: height,
            kIOSurfaceBytesPerElement: 4,
            kIOSurfacePixelFormat: kCVPixelFormatType_32BGRA,
        ]
        guard width > 0, height > 0, let surface = IOSurfaceCreate(properties as CFDictionary) else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm, width: width, height: height, mipmapped: false)
        descriptor.usage = [.renderTarget]
        guard let texture = device.makeTexture(descriptor: descriptor, iosurface: surface, plane: 0) else { return nil }
        return Surface(surface: surface, texture: texture)
    }

    /// Swaps a desktop's picture for one half its size: out of sight on another Space it is only seen sliding by.
    private func shrink(_ desktop: Desktop) {
        guard !desktop.shrunk, !desktop.drawing, let front = desktop.front else { return }
        let width = front.texture.width / 2, height = front.texture.height / 2
        guard let small = makeSurface(width: width, height: height) else { return }
        IOSurfaceLock(front.surface, .readOnly, nil)
        IOSurfaceLock(small.surface, [], nil)
        var source = vImage_Buffer(data: IOSurfaceGetBaseAddress(front.surface), height: vImagePixelCount(front.texture.height),
                                   width: vImagePixelCount(front.texture.width), rowBytes: IOSurfaceGetBytesPerRow(front.surface))
        var target = vImage_Buffer(data: IOSurfaceGetBaseAddress(small.surface), height: vImagePixelCount(height), width: vImagePixelCount(width),
                                   rowBytes: IOSurfaceGetBytesPerRow(small.surface))
        let scaled = vImageScale_ARGB8888(&source, &target, nil, vImage_Flags(kvImageNoFlags))
        IOSurfaceUnlock(small.surface, [], nil)
        IOSurfaceUnlock(front.surface, .readOnly, nil)
        guard scaled == kvImageNoError else { return }
        desktop.window.snow.contents = small.surface
        desktop.front = small
        desktop.shrunk = true
    }
}
