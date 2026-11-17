import AppKit
import simd

/// A snowball fight with a snowman living in the wallpaper's picture: for now a bot, before there is anyone to play
/// with. He walks about and throws snowballs out of the picture at the screen; Option-click the wallpaper to throw
/// one back into it.
final class SnowballFight {
    /// Lets it be tried out: defaults write com.mbezruchko.Snowhush previewSnowballFight -bool true
    static var wanted: Bool { UserDefaults.standard.bool(forKey: "previewSnowballFight") }

    private let renderer: SnowmanRenderer
    private(set) var bot: SnowmanBot?
    private weak var window: PictureWindow?
    private var groundID: Int?
    private var steppedAt: Double = 0
    private var drawn = false
    private var monitor: Any?
    /// Told of each Option-click anywhere, where it was on the screens.
    var clicked: (NSPoint) -> Void = { _ in }

    init?(device: MTLDevice) {
        guard let renderer = SnowmanRenderer(device: device) else { return nil }
        self.renderer = renderer
        // Clicks go on to whatever they were for; one with Option held over the wallpaper also throws.
        monitor = NSEvent.addGlobalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard event.modifierFlags.contains(.option) else { return }
            self?.clicked(NSEvent.mouseLocation)
        }
    }

    deinit {
        if let monitor { NSEvent.removeMonitor(monitor) }
        renderer.layer.removeFromSuperlayer()
    }

    /// Puts the snowman in `window`'s picture, on `ground`; a new one when either changed.
    func place(in window: PictureWindow, ground: SnowmanGround, now: Double) {
        if window !== self.window {
            renderer.layer.removeFromSuperlayer()
            window.contentView?.layer?.addSublayer(renderer.layer)
            self.window = window
            drawn = false
        }
        if ground.id != groundID {
            groundID = ground.id
            bot = SnowmanBot(ground: ground, now: now)
            drawn = false
        }
    }

    /// Moves the snowman on and draws him. `covers` are the windows in front of the picture; he throws only where he
    /// can be seen.
    func step(now: Double, relief: MTLTexture, screen: SIMD2<Float>, scale: CGFloat, covers: [SIMD4<Float>], visible: Bool, simulation: SnowSimulation) {
        let dt = steppedAt > 0 ? Float(min(now - steppedAt, 0.05)) : 1.0 / 60
        steppedAt = now
        guard let bot else { return }
        let reach = bot.reach
        let covered = covers.contains { $0.x < reach.z && $0.z > reach.x && $0.y < reach.w && $0.w > reach.y }
        let moved = bot.step(dt: dt, now: now, canThrow: visible && !covered) { hand, size in
            simulation.throwAtScreen(from: hand, size: size)
        }
        if moved || !drawn {
            renderer.draw(bot, relief: relief, screen: screen, scale: scale)
            drawn = true
        }
    }

    /// Throws a snowball from in front of the screen at `point` in the picture, where it is `near`; it hits the
    /// snowman if he is still there when it comes down.
    func throwBall(at point: SIMD2<Float>, near: Float, size: Float, screenWidth: Float, simulation: SnowSimulation) {
        let start = SIMD2(screenWidth * 0.5 + (point.x - screenWidth * 0.5) * 0.35, -30)
        simulation.throwIntoPicture(from: start, to: point, near: near, size: size) { [weak self, unowned simulation] landed in
            guard let bot = self?.bot, bot.contains(landed), abs(bot.near - near) < 0.08 else { return false }
            bot.hit(at: landed, now: CACurrentMediaTime())
            simulation.puff(at: landed, near: bot.near, size: size * 1.4)
            return true
        }
    }
}
