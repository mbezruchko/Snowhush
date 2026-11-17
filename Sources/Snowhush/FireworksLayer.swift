import AppKit
import Metal
import QuartzCore

/// The fireworks drawn into the wallpaper's own window, at the desktop's level: every window and widget is in front of
/// them by the window server's own doing, as of the wallpaper itself. Its frames are held only while sparks fly.
final class FireworksLayer {
    private let queue: MTLCommandQueue
    private let renderer: TreeRenderer
    private let drawables: DrawableLayer
    var layer: CAMetalLayer { drawables.layer }
    private var shown = false

    init?(device: MTLDevice) {
        guard let queue = device.makeCommandQueue(), let renderer = try? TreeRenderer(device: device, queue: queue, pixelFormat: .bgra8Unorm) else { return nil }
        self.queue = queue
        self.renderer = renderer
        let layer = CAMetalLayer()
        layer.device = device
        layer.pixelFormat = .bgra8Unorm
        layer.isOpaque = false
        layer.framebufferOnly = true
        layer.contentsScale = 1
        layer.actions = ["position": NSNull(), "bounds": NSNull(), "frame": NSNull(), "contents": NSNull()]
        drawables = DrawableLayer(layer)
    }

    /// Draws the sparks over `size` points, hidden behind whatever in the picture of `relief` is nearer than they are;
    /// with none left, clears once and lets its frames go.
    func draw(_ fireworks: Fireworks?, size: CGSize, relief: MTLTexture?, time: Float) {
        let sparks = !(fireworks?.isEmpty ?? true)
        guard sparks || shown else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let wanted = sparks ? size : CGSize(width: 1, height: 1)
        if layer.drawableSize != wanted { layer.drawableSize = wanted }
        guard let drawable = drawables.nextDrawable(), let commandBuffer = queue.makeCommandBuffer() else { return }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = drawable.texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 0)
        pass.colorAttachments[0].storeAction = .store
        guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else { return }
        if let fireworks, sparks {
            renderer.encode(fireworks, with: encoder, uniforms: TreeUniforms(origin: .zero, size: SIMD2(Float(size.width), Float(size.height)), time: time),
                            air: relief, covers: [])
        }
        encoder.endEncoding()
        commandBuffer.present(drawable)
        commandBuffer.commit()
        shown = sparks
    }
}
