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

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    // Borderless windows are otherwise pushed below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}
