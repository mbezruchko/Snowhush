import AppKit
import QuartzCore

/// The snow lying on the Dock, drawn in a strip along the bottom of the screen just below the Dock's level:
/// the main overlay sits above the Dock, where the snow would cover icons that grow under the pointer.
/// The strip shares the main overlay's coordinates, with its origin at the bottom left of the screen.
final class DockOverlay {
    let window: OverlayWindow
    private let layer = CAMetalLayer()
    private var screenFrame: CGRect
    /// The strip's height; it grows to fit the snow and shrinks back as the snow goes.
    private(set) var height: CGFloat = 1

    init(screen: NSScreen, device: MTLDevice) {
        screenFrame = screen.frame
        window = OverlayWindow(screen: screen, level: OverlayWindow.belowDockLevel)
        layer.device = device
        layer.pixelFormat = .bgra8Unorm
        layer.isOpaque = false
        layer.framebufferOnly = true
        // While the strip resizes, a frame of the other size stays pinned to the shared origin instead of stretching.
        layer.contentsGravity = .bottomLeft
        layer.needsDisplayOnBoundsChange = false
        let view = NSView(frame: CGRect(origin: .zero, size: screen.frame.size))
        view.layer = layer
        view.wantsLayer = true
        view.autoresizingMask = [.width, .height]
        window.contentView = view
        place()
        window.orderFrontRegardless()
    }

    func fit(to screen: NSScreen) {
        screenFrame = screen.frame
        place()
    }

    /// The next frame to draw into, cleared, with the strip resized first to reach `top` when given.
    func nextFrame(reaching top: CGFloat?) -> (drawable: CAMetalDrawable, pass: MTLRenderPassDescriptor)? {
        if let top {
            // Steps of 64 points keep a growing drift from resizing the window every frame.
            let needed = min(screenFrame.height, (max(top, 1) / 64).rounded(.up) * 64 + 32)
            if needed > height || needed < height - 128 {
                height = needed
                place()
            }
        }
        guard let drawable = layer.nextDrawable() else { return nil }
        let pass = MTLRenderPassDescriptor()
        let attachment = pass.colorAttachments[0]!
        attachment.texture = drawable.texture
        attachment.loadAction = .clear
        attachment.storeAction = .store
        attachment.clearColor = MTLClearColorMake(0, 0, 0, 0)
        return (drawable, pass)
    }

    func close() {
        window.orderOut(nil)
    }

    private func place() {
        window.setFrame(CGRect(x: screenFrame.minX, y: screenFrame.minY, width: screenFrame.width, height: height), display: false)
        // One pixel per point, as for the main overlay.
        layer.drawableSize = CGSize(width: screenFrame.width, height: height)
    }
}
