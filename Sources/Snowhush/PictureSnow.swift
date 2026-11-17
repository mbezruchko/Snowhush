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
        /// What the system last named as its wallpaper, once it has been asked, and how many times it has been:
        /// what an earlier asking finds out late is dropped.
        var asked: Wallpaper.Asked?
        var known = false
        var asking = 0
        /// When its Space last came to be the one showing, and what the system named for it then that it had not
        /// before, to be believed once it names it again.
        var shownAt: CFTimeInterval = 0
        var doubt: Wallpaper.Asked?
        var doubting = false
        /// How much snow lay in its picture when its Space was last left, and when; nil while it is the one showing,
        /// whose snow the simulation keeps, or has never been.
        var snow: Float?
        var leftAt: CFTimeInterval = 0
        /// Whether its wallpaper has been read, and how near the things in it are, for the flakes; the rest of what
        /// was read the renderer has, for as long as this is the desktop it draws.
        var read = false
        var terrain: WallpaperTerrain?
        var light: WallpaperLight?
        var front: Surface?
        var level: Float = -1
        var veil: Float = -1
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
    /// The snow's level as last kept for the next run of the app, and when the display last went dark or away.
    private var kept: Float = 0
    private var restedAt: CFTimeInterval?
    /// A wallpaper is being read.
    private var reading = false
    /// Whether the system has been asked for the wallpaper since the Space or the screen last changed, when, and the
    /// date its store of wallpapers had then.
    private var asked = false
    private var askedAt: CFTimeInterval = 0
    private var storeStamp: Int?
    private static let store = NSHomeDirectory() + "/Library/Application Support/com.apple.wallpaper/Store/Index.plist"
    /// Tells the moment that store is rewritten: a look a second apart would leave the old wallpaper's snow lying on
    /// the new one that long.
    private var storeWatch: DispatchSourceFileSystemObject?

    /// One wallpaper at a time is looked at, for all the displays: each look holds a few screens' worth of pixels.
    private static let lookout = DispatchQueue(label: "com.mbezruchko.Snowhush.wallpaper", qos: .utility)
    /// A wallpaper's file is found on a queue of its own: behind a wallpaper being read it would wait seconds.
    private static let files = DispatchQueue(label: "com.mbezruchko.Snowhush.wallpaper.files", qos: .userInitiated)
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
                storeWatch?.cancel()
                storeWatch = nil
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
        kept = Preferences.pictureLevel
        simulation.restorePicture(level: kept)
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
        guard active != self.active else { return }
        self.active = active
        if active {
            // It went on snowing while no one was looking.
            if let restedAt, wanted { simulation.passPicture(seconds: Float(CACurrentMediaTime() - restedAt)) }
            restedAt = nil
            wake()
        } else {
            restedAt = CACurrentMediaTime()
            keep()
            rest()
        }
    }

    /// Keeps the snow's level for the next run of the app.
    private func keep() {
        guard wanted else { return }
        kept = simulation.pictureLevel
        Preferences.pictureLevel = kept
    }

    /// Starts looking again, a second apart: for another wallpaper, and for snow to draw.
    func wake() {
        guard wanted, active, timer == nil else { return }
        watchStore()
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
        storeWatch?.cancel()
        storeWatch = nil
        for desktop in desktops.values { close(desktop) }
        desktops = [:]
    }

    private func watchStore() {
        guard storeWatch == nil else { return }
        // The folder, not the file: the file is replaced by another each time, and the folder is what changes then.
        let descriptor = open((Self.store as NSString).deletingLastPathComponent, O_EVTONLY)
        guard descriptor >= 0 else { return }
        let watch = DispatchSource.makeFileSystemObjectSource(fileDescriptor: descriptor, eventMask: [.write, .rename, .delete, .attrib, .extend], queue: .main)
        watch.setEventHandler { [weak self] in self?.look() }
        watch.setCancelHandler { Darwin.close(descriptor) }
        watch.resume()
        storeWatch = watch
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
    /// Flakes have come down in the picture: the frost they leave shows there now, not at the next look.
    func landed() {
        guard wanted, active, let desktop = current, desktop.terrain != nil, loaded == desktop.picture, renderer.loaded, !desktop.drawing,
              desktop.dabs != simulation.dabsVersion, desktop.window.occlusionState.contains(.visible) else { return }
        draw(desktop, level: simulation.pictureLevel, veil: simulation.pictureVeil, now: CACurrentMediaTime())
    }

    func cleared() {
        kept = 0
        Preferences.pictureLevel = 0
        for desktop in desktops.values where desktop !== current {
            clear(desktop)
            if desktop.snow != nil { desktop.snow = 0 }
        }
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
        let rewritten = storeStamp != nil && stamp != storeStamp
        asked = true
        askedAt = now
        storeStamp = stamp
        // The system's word on which wallpaper shows could come a moment after its store is rewritten: it is asked a
        // few more times over the next seconds, so that a wallpaper named late is not found half a minute later.
        if rewritten { for delay in [0.25, 0.8, 2.0] { askAgain(in: delay) } }
        return true
    }

    private func askAgain(in delay: Double) {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self else { return }
            self.asked = false
            self.look()
        }
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

    /// Finds which wallpaper a desktop shows. The system is asked here; the picture's file is looked at off the main
    /// thread, since a first look into a folder the system guards waits for the user to allow it, and nothing else
    /// must wait with it.
    private func ask(for desktop: Desktop) {
        let asked = Wallpaper.asked(on: screen)
        if desktop.known, asked == desktop.asked {
            desktop.doubting = false
        } else {
            // On a Space just come to the system may for a moment still name the wallpaper of the one left: there it
            // is believed only once it has said the same twice, and a wallpaper found to be another is the same
            // picture as ever, with its snow.
            let arriving = CACurrentMediaTime() - desktop.shownAt < 1.5
            if arriving, !(desktop.doubting && desktop.doubt == asked) {
                desktop.doubting = true
                desktop.doubt = asked
                askAgain(in: 0.3)
                return
            }
            desktop.doubting = false
            // The moment the system names another wallpaper the old one's snow goes, before the new one's file is so
            // much as found.
            if desktop.known { retire(desktop, startingOver: !arriving && asked?.url != desktop.asked?.url) }
            desktop.known = true
            desktop.asked = asked
        }
        desktop.asking += 1
        let turn = desktop.asking
        guard let asked else {
            show(nil, on: desktop)
            return
        }
        Self.files.async { [weak self, weak desktop] in
            let picture = Wallpaper.picture(asked)
            DispatchQueue.main.async {
                guard let self, let desktop, desktop.asking == turn else { return }
                self.show(picture, on: desktop)
                self.look()
            }
        }
    }

    /// A desktop's wallpaper is `picture` now.
    private func show(_ picture: Wallpaper.Picture?, on desktop: Desktop) {
        guard picture != desktop.picture else { return }
        // Under the same name and laid out the same, yet another file: another picture all the same.
        retire(desktop, startingOver: picture != nil && desktop.picture != nil)
        desktop.picture = picture
    }

    /// The wallpaper a desktop showed is gone from it, for another or for the same one laid out anew: its snow goes
    /// with it at once. On another picture the snow starts over, and forms on it from the first flake.
    private func retire(_ desktop: Desktop, startingOver: Bool) {
        desktop.picture = nil
        desktop.read = false
        desktop.terrain = nil
        desktop.light = nil
        fade(desktop.window.snow, over: 0.3)
        clear(desktop)
        if desktop === current {
            simulation.setPicture(nil)
            if startingOver {
                simulation.restorePicture(level: 0)
                keep()
            }
        } else if startingOver {
            desktop.snow = 0
            desktop.leftAt = CACurrentMediaTime()
        }
    }

    /// Looks at the desktop showing: for another wallpaper on it, which is then read off the main thread, and for
    /// snow that has changed, which is drawn. With nothing falling, lying or melting it rests until woken.
    private func look() {
        guard wanted, active, let desktop = desktopShowing() else { return }
        if desktop !== current {
            // Each desktop has snow of its own: the one left keeps what lay in it, and the one come to has been
            // snowed on for as long as it was away. One never shown before starts with what lies in this one.
            let now = CACurrentMediaTime()
            if let left = current {
                left.snow = simulation.pictureLevel
                left.leftAt = now
            }
            let came = current != nil
            current = desktop
            if came { desktop.shownAt = now }
            if let snow = desktop.snow {
                simulation.restorePicture(level: simulation.snowInPicture(from: snow, after: Float(min(now - desktop.leftAt, 8 * 3600))))
                desktop.snow = nil
            }
            simulation.setPicture(desktop.terrain, light: desktop.light)
            // Asking the system which wallpaper shows takes tens of milliseconds while a Space slides in, enough to
            // make the falling snow stutter, and what it names then may still be the wallpaper of the Space left: it
            // is asked once the Space is there.
            if came {
                asked = true
                askedAt = now
                askAgain(in: 0.8)
            } else {
                asked = false
            }
        }
        if wallpaperMayHaveChanged() { ask(for: desktop) }
        // The wallpaper is read when it is new, and again when the renderer has since drawn another desktop's: off
        // the main thread, by another run of the app the first time and from what that kept after.
        if let picture = desktop.picture, !reading, !desktop.read || (desktop.terrain != nil && loaded != picture) {
            reading = true
            let columnWidth = simulation.columnWidth
            Self.lookout.async { [weak self, weak desktop] in
                let scene = Wallpaper.sceneReadApart(in: picture, columnWidth: columnWidth)
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.reading = false
                    // Read for a wallpaper the desktop no longer shows, it is dropped, and the one it shows now read.
                    if let desktop, desktop.picture == picture {
                        desktop.read = true
                        desktop.terrain = scene?.terrain
                        desktop.light = scene?.light
                        if desktop === self.current {
                            self.renderer.load(scene, columnWidth: columnWidth)
                            self.loaded = scene == nil ? nil : picture
                            self.simulation.setPicture(scene?.terrain, light: scene?.light)
                            desktop.level = -1
                        }
                    }
                    self.look()
                }
            }
        }
        guard desktop.terrain != nil, loaded == desktop.picture, renderer.loaded, !desktop.drawing else { return }
        let level = simulation.pictureLevel, veil = simulation.pictureVeil
        if abs(level - kept) >= 0.02 { keep() }
        guard level > 0 || simulation.hasDabs || veil > 0 else {
            if desktop.front != nil { clear(desktop) }
            if simulation.isEmpty { rest() }
            return
        }
        let now = CACurrentMediaTime()
        // What the flakes seen landing add in a second can't be told; in a few it can.
        let changed = abs(level - desktop.level) >= Self.step || abs(veil - desktop.veil) >= 0.01 || desktop.shrunk
            || (desktop.dabs != simulation.dabsVersion && now - desktop.drawnAt >= 1)
        // Behind other windows it is drawn now and then all the same, so that uncovered it is never far behind.
        guard changed, desktop.window.occlusionState.contains(.visible) || now - desktop.drawnAt > 15 else { return }
        draw(desktop, level: level, veil: veil, now: now)
    }

    private func draw(_ desktop: Desktop, level: Float, veil: Float, now: CFTimeInterval) {
        // Each picture is drawn into a surface of its own, and the one before let go once this one shows: drawing into
        // the one on show would flicker, and keeping a second for next time would double the memory.
        let size = screen.frame.size
        guard let target = makeSurface(width: Int(size.width), height: Int(size.height)) else { return }
        let dabs = simulation.dabsVersion
        let first = desktop.front == nil
        desktop.drawing = true
        let drawn = renderer.draw(level: level, veil: veil, dabs: simulation.dabs, specks: simulation.specks, dabsSize: simulation.dabsSize, dabsVersion: dabs, into: target.texture,
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
            desktop.veil = veil
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
