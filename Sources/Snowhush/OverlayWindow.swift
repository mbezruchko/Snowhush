import AppKit

final class OverlayWindow: NSWindow {
    private static let desktopLevel = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.desktopWindow)) + 1)
    /// Just under the Dock, so its icons stay in front as they grow under the pointer.
    static let belowDockLevel = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.dockWindow)) - 1)

    private let frontLevel: NSWindow.Level

    var behindWindows = false {
        didSet { level = behindWindows ? Self.desktopLevel : frontLevel }
    }

    var showsInScreenshots = false {
        didSet { sharingType = showsInScreenshots ? .readOnly : .none }
    }

    init(screen: NSScreen, level: NSWindow.Level = .statusBar) {
        frontLevel = level
        super.init(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        ignoresMouseEvents = true
        isReleasedWhenClosed = false
        animationBehavior = .none
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle]
        self.level = level
        sharingType = .none
        setFrame(screen.frame, display: false)
    }

    /// Ordering a window front puts it back on the current Space, so it goes back into the fixed one each time.
    override func orderFrontRegardless() {
        super.orderFrontRegardless()
        holdInPlace()
    }

    /// Keeps the overlay out of the user's Spaces, so a switch between them doesn't carry it off with their windows.
    func holdInPlace() {
        FixedSpace.hold(self)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    // Borderless windows are otherwise pushed below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}

/// A Space of the app's own that stays on screen through switches between the user's Spaces, the way the Dock and the
/// menu bar do: windows held in it don't slide off with the desktop. Through private SkyLight calls; without them the
/// overlays stay on every Space as before, and slide with them.
enum FixedSpace {
    private typealias Create = @convention(c) (Int32, Int32, CFDictionary?) -> UInt64
    private typealias SetLevel = @convention(c) (Int32, UInt64, Int32) -> Int32
    private typealias Show = @convention(c) (Int32, CFArray) -> Int32
    private typealias Claim = @convention(c) (Int32, UInt64, CFArray, Int32) -> Int32

    private static let claim = SkyLight.function("SLSSpaceAddWindowsAndRemoveFromSpaces", as: Claim.self)

    private static let space: UInt64? = {
        guard let connection = SkyLight.connection,
              let create = SkyLight.function("SLSSpaceCreate", as: Create.self),
              let show = SkyLight.function("SLSShowSpaces", as: Show.self)
        else { return nil }
        // 1 keeps Finder from drawing the desktop's icons into it.
        let space = create(connection, 1, nil)
        guard space != 0 else { return nil }
        _ = SkyLight.function("SLSSpaceSetAbsoluteLevel", as: SetLevel.self)?(connection, space, 0)
        _ = show(connection, [NSNumber(value: space)] as CFArray)
        return space
    }()

    static func hold(_ window: NSWindow) {
        guard let connection = SkyLight.connection, let space, let claim, window.windowNumber > 0 else { return }
        // 7 takes it off every other Space: the current one, the others and the user's.
        _ = claim(connection, space, [NSNumber(value: window.windowNumber)] as CFArray, 7)
    }
}
