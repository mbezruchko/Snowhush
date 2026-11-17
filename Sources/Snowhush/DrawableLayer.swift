import QuartzCore

/// A Metal layer whose drawables never keep the main thread waiting. While a Space slides in or away, the window
/// server gives back none of the drawables of the desktop-level windows' layers, and `nextDrawable` then blocks for a
/// second at a time: each drawable is asked for ahead, off the main thread, and a frame with none there yet is let go.
final class DrawableLayer {
    let layer: CAMetalLayer
    private let lock = NSLock()
    /// The drawable asked for ahead, and whether one is being asked for.
    private var ready: CAMetalDrawable?
    private var asking = false
    private static let asked = DispatchQueue(label: "com.mbezruchko.Snowhush.drawables", qos: .userInteractive, attributes: .concurrent)

    init(_ layer: CAMetalLayer) {
        self.layer = layer
    }

    /// The drawable asked for ahead, if it has come and is of the layer's size now; the next is asked for at once.
    func nextDrawable() -> CAMetalDrawable? {
        var taken: CAMetalDrawable?
        var ask = false
        lock.withLock {
            taken = ready
            ready = nil
            if !asking {
                asking = true
                ask = true
            }
        }
        if ask {
            let layer = layer
            Self.asked.async { [weak self] in
                let drawable = layer.nextDrawable()
                guard let self else { return }
                lock.withLock {
                    self.ready = drawable
                    self.asking = false
                }
            }
        }
        guard let taken, CGSize(width: taken.texture.width, height: taken.texture.height) == layer.drawableSize else { return nil }
        return taken
    }
}
