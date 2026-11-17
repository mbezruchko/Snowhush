import AppKit
import simd

/// A snowball fight with snowmen living in the wallpaper's picture: the friends in this player's room, or a bot while
/// there is no one to play with. They walk about and throw snowballs out of the picture at the screen: the bot only
/// back at whoever hits it, never first; a friend when the friend hits this player. Command-click the wallpaper to
/// throw one into it: hitting a friend's snowman sends the friend a snowball. Each snowman takes five hits before he
/// slumps away; in a room the last one standing wins the round.
final class SnowballFight {
    /// Lets it be tried out: defaults write com.mbezruchko.Snowhush previewSnowballFight -bool true, or a build handed
    /// to friends with SnowballFight set in its Info.plist.
    static var wanted: Bool {
        UserDefaults.standard.bool(forKey: "previewSnowballFight") || Bundle.main.object(forInfoDictionaryKey: "SnowballFight") as? Bool == true
    }

    private final class Figure {
        let bot: SnowmanBot
        let renderer: SnowmanRenderer
        /// A friend's name and what is left of his snowman; none over the bot.
        let label: CATextLayer?
        let name: String?
        var shown = ""
        var drawn = false
        /// Hearts he had as last seen; and since when he owes a move elsewhere for one lost, until the snowball that
        /// cost it is seen to land.
        var hearts = SnowballRoom.hearts
        var moveOwed: Double?

        init(bot: SnowmanBot, renderer: SnowmanRenderer, name: String?) {
            self.bot = bot
            self.renderer = renderer
            self.name = name
            label = name.map { name in
                let label = CATextLayer()
                label.string = name
                label.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
                label.fontSize = 11
                label.foregroundColor = NSColor(white: 1, alpha: 0.92).cgColor
                label.alignmentMode = .center
                label.shadowColor = NSColor.black.cgColor
                label.shadowOpacity = 0.6
                label.shadowRadius = 2
                label.shadowOffset = .zero
                label.actions = ["position": NSNull(), "bounds": NSNull(), "hidden": NSNull(), "contents": NSNull()]
                label.bounds = CGRect(x: 0, y: 0, width: 160, height: 15)
                return label
            }
        }
    }

    private let device: MTLDevice
    /// Players' figures by their ids; the bot's under `botID`.
    private var figures: [String: Figure] = [:]
    private static let botID = "bot"
    private weak var window: PictureWindow?
    private var ground: SnowmanGround?
    private var steppedAt: Double = 0
    private var monitor: Any?
    private var observer: NSObjectProtocol?
    private var roomObserver: NSObjectProtocol?
    private weak var simulation: SnowSimulation?
    /// What is left of the bot, and when he comes back once it is gone.
    private var botLives = SnowballFight.botHits
    /// Hits the bot takes before he slumps away: he has no hearts to show, and alone fifteen would drag.
    private static let botHits = 5
    private var botBackAt: Double?
    /// The round as this player sees it, at the top of the desktop while in a room: own hearts, wins, who won.
    private let board = CATextLayer()
    private var boardShown = ""
    /// Told of each Command-click anywhere, where it was on the screens.
    var clicked: (NSPoint) -> Void = { _ in }
    /// The fight on the main display: it alone answers what happens in the room, so that a friend throws once.
    var main = true

    init(device: MTLDevice, simulation: SnowSimulation) {
        self.device = device
        self.simulation = simulation
        // Clicks go on to whatever they were for; one with Command held over the wallpaper also throws.
        monitor = NSEvent.addGlobalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard event.modifierFlags.contains(.command) else { return }
            self?.clicked(NSEvent.mouseLocation)
        }
        // The round shows as it changes, whether or not anything is moving.
        roomObserver = NotificationCenter.default.addObserver(forName: SnowballRoom.changed, object: nil, queue: .main) { [weak self] _ in
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            self?.showBoard()
            CATransaction.commit()
        }
        observer = NotificationCenter.default.addObserver(forName: SnowballRoom.thrown, object: nil, queue: .main) { [weak self] note in
            guard let from = note.userInfo?["from"] as? String, let at = note.userInfo?["at"] as? String else { return }
            self?.thrown(from: from, at: at)
        }
    }

    deinit {
        if let monitor { NSEvent.removeMonitor(monitor) }
        if let observer { NotificationCenter.default.removeObserver(observer) }
        if let roomObserver { NotificationCenter.default.removeObserver(roomObserver) }
        for figure in figures.values { remove(figure) }
        board.removeFromSuperlayer()
    }

    private func remove(_ figure: Figure) {
        figure.renderer.layer.removeFromSuperlayer()
        figure.label?.removeFromSuperlayer()
    }

    /// In a wallpaper with no ground to stand on: no snowmen, only the round at the top of `window`.
    func showRound(in window: PictureWindow) {
        if window !== self.window || ground != nil {
            for figure in figures.values { remove(figure) }
            figures = [:]
            ground = nil
            self.window = window
            board.removeFromSuperlayer()
            boardShown = ""
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        showBoard()
        CATransaction.commit()
    }

    /// Puts the snowmen in `window`'s picture, on `ground`: the friends in the room, or the bot when there are none.
    func place(in window: PictureWindow, ground: SnowmanGround, now: Double) {
        if ground.id != self.ground?.id {
            for figure in figures.values { remove(figure) }
            figures = [:]
            self.ground = ground
        }
        if window !== self.window {
            self.window = window
            board.removeFromSuperlayer()
            boardShown = ""
            for figure in figures.values {
                remove(figure)
                attach(figure)
            }
        }
        let room = SnowballRoom.shared
        var wanted: [String: String?] = [:]
        if room.me != nil {
            if main { for player in room.players { wanted[player.id] = player.name } }
        } else {
            wanted[Self.botID] = .some(nil)
        }
        for (id, figure) in figures where wanted[id] == nil {
            remove(figure)
            figures[id] = nil
        }
        for (id, name) in wanted where figures[id] == nil {
            guard let renderer = SnowmanRenderer(device: device),
                  let bot = SnowmanBot(ground: ground, now: now, throwsOnHisOwn: id == Self.botID) else { continue }
            let figure = Figure(bot: bot, renderer: renderer, name: name)
            figures[id] = figure
            attach(figure)
        }
        // Each friend's snowman as the round has him: what is left of him beside his name, gone once he is out.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for (id, figure) in figures where id != Self.botID {
            noteHearts(of: figure, id: id, now: now)
            if room.isOut(id) {
                figure.bot.knockOut()
            } else {
                figure.bot.comeBack()
            }
            guard let label = figure.label, let name = figure.name else { continue }
            let lives = room.lives(of: id)
            let key = "\(name)|\(lives)"
            if key != figure.shown {
                figure.shown = key
                let font = NSFont.systemFont(ofSize: 11, weight: .semibold)
                let text = NSMutableAttributedString(string: name + " ", attributes: [.font: font, .foregroundColor: NSColor(white: 1, alpha: 0.92)])
                text.append(Self.hearts(lives, color: .white, font: font))
                label.string = text
            }
            label.isHidden = room.isOut(id)
        }
        showBoard()
        CATransaction.commit()
    }

    /// A friend's snowman that has lost a heart is owed a move elsewhere; one out of the round, or whole again in a new
    /// one, none.
    private func noteHearts(of figure: Figure, id: String, now: Double) {
        let room = SnowballRoom.shared
        let hearts = room.hearts(of: id)
        if room.isOut(id) || hearts > figure.hearts {
            figure.moveOwed = nil
        } else if hearts < figure.hearts, figure.moveOwed == nil {
            figure.moveOwed = now
        }
        figure.hearts = hearts
    }

    /// He took the snowball that cost him a heart, seen to land: he moves elsewhere.
    private func moveIfOwed(_ figure: Figure) {
        guard figure.moveOwed != nil else { return }
        figure.moveOwed = nil
        figure.bot.moveAway()
    }

    /// Five hearts: full for each whole one left, fainter for one that has taken a hit or two, an outline for each
    /// one gone.
    private static func hearts(_ lives: Int, color: NSColor, font: NSFont) -> NSAttributedString {
        let text = NSMutableAttributedString()
        let per = SnowballRoom.hitsPerHeart
        for k in 0..<SnowballRoom.hearts {
            let left = min(max(lives - k * per, 0), per)
            let glyph = left > 0 ? "♥" : "♡"
            let alpha = left == per || left == 0 ? 1 : 0.3 + 0.4 * CGFloat(left) / CGFloat(per)
            text.append(NSAttributedString(string: glyph, attributes: [.font: font, .foregroundColor: color.withAlphaComponent(alpha)]))
        }
        return text
    }

    /// Snowballs this player has thrown, kept to tell how to throw until he has a few times.
    private static var throwsMade: Int {
        get { UserDefaults.standard.integer(forKey: "fightThrows") }
        set { UserDefaults.standard.set(newValue, forKey: "fightThrows") }
    }

    /// The round at the top of the desktop: who won it, or this player's hearts or that he is out, and everyone's wins;
    /// and how to throw, until he has thrown a few.
    private func showBoard() {
        let room = SnowballRoom.shared
        let hint = Self.throwsMade < 3 ? tr("fight.hint") : ""
        guard main, let window, room.me != nil || !hint.isEmpty else {
            if boardShown != "" {
                boardShown = ""
                board.removeFromSuperlayer()
            }
            return
        }
        guard let me = room.me else {
            showLines(nil, wins: "", hint: hint, in: window)
            return
        }
        let headline: String
        if let winner = room.winner {
            headline = winner == me ? tr("fight.youwon") : String(format: tr("fight.won"), room.name(of: winner))
        } else if room.isOut(me) {
            headline = tr("fight.out")
        } else {
            headline = "♥\(room.lives(of: me))"
        }
        let everyone = [me] + room.players.map(\.id)
        let wins = everyone.contains { room.wins(of: $0) > 0 }
            ? "\(tr("fight.wins")): " + everyone.map { "\(room.name(of: $0)) \(room.wins(of: $0))" }.joined(separator: " · ") : ""
        let big = room.winner != nil
        let font = NSFont.systemFont(ofSize: big ? 26 : 18, weight: .bold)
        let pink = NSColor(srgbRed: 1, green: 0.45, blue: 0.5, alpha: 1)
        let line = room.winner == nil && !room.isOut(me)
            ? NSMutableAttributedString(attributedString: Self.hearts(room.lives(of: me), color: pink, font: font))
            : NSMutableAttributedString(string: headline, attributes: [.font: font, .foregroundColor: big ? NSColor.white : pink])
        showLines(line, wins: wins, hint: hint, in: window, key: headline + "|" + String(room.lives(of: me)))
    }

    private func showLines(_ headline: NSAttributedString?, wins: String, hint: String, in window: PictureWindow, key: String = "") {
        let key = key + "|" + wins + "|" + hint
        if board.superlayer == nil, let host = window.contentView?.layer {
            board.contentsScale = window.backingScaleFactor
            board.alignmentMode = .center
            board.shadowColor = NSColor.black.cgColor
            board.shadowOpacity = 0.6
            board.shadowRadius = 3
            board.shadowOffset = .zero
            board.actions = ["position": NSNull(), "bounds": NSNull(), "contents": NSNull(), "hidden": NSNull()]
            host.addSublayer(board)
            boardShown = ""
        }
        guard key != boardShown else { return }
        boardShown = key
        let text = NSMutableAttributedString()
        if let headline { text.append(headline) }
        for (line, size) in [(wins, CGFloat(12)), (hint, CGFloat(13))] where !line.isEmpty {
            text.append(NSAttributedString(string: (text.length > 0 ? "\n" : "") + line, attributes: [
                .font: NSFont.systemFont(ofSize: size, weight: .semibold), .foregroundColor: NSColor(white: 1, alpha: 0.9),
            ]))
        }
        board.string = text
        let size = window.frame.size
        board.frame = CGRect(x: 0, y: size.height - 120, width: size.width, height: 80)
    }

    private func attach(_ figure: Figure) {
        guard let host = window?.contentView?.layer else { return }
        host.addSublayer(figure.renderer.layer)
        if let label = figure.label {
            label.contentsScale = window?.backingScaleFactor ?? 2
            host.addSublayer(label)
        }
        figure.drawn = false
    }

    /// Moves the snowmen on and draws them. `covers` are the windows in front of the picture; the bot throws only
    /// where it can be seen.
    func step(now: Double, relief: MTLTexture, screen: SIMD2<Float>, scale: CGFloat, covers: [SIMD4<Float>], visible: Bool) {
        let dt = steppedAt > 0 ? Float(min(now - steppedAt, 0.05)) : 1.0 / 60
        steppedAt = now
        guard let simulation else { return }
        if let back = botBackAt, now >= back, let bot = figures[Self.botID]?.bot {
            botBackAt = nil
            botLives = Self.botHits
            bot.comeBack()
        }
        for figure in figures.values {
            // The snowball that cost him a heart may never be seen to land, its thrower gone: he moves all the same.
            if let owed = figure.moveOwed, now - owed > 3 { moveIfOwed(figure) }
            let reach = figure.bot.reach
            let covered = covers.contains { $0.x < reach.z && $0.z > reach.x && $0.y < reach.w && $0.w > reach.y }
            let moved = figure.bot.step(dt: dt, now: now, canThrow: visible && !covered) { [weak self] hand, size, target in
                guard let target else {
                    simulation.throwAtScreen(from: hand, size: size, near: figure.bot.near)
                    return
                }
                // At a friend's snowman, across the picture.
                guard let struck = self?.figures[target] else { return }
                let other = struck.bot
                simulation.throwIntoPicture(from: hand, to: other.middle, near: other.near, size: other.height * 0.1, startSize: size, from: figure.bot.near) { [weak self] landed in
                    other.hit(at: landed, now: CACurrentMediaTime())
                    simulation.puff(at: landed, near: other.near, size: other.height * 0.14)
                    self?.moveIfOwed(struck)
                    return true
                }
            }
            for (point, size) in figure.bot.takeBursts() {
                simulation.crumble(at: point, floor: figure.bot.foot.y, near: figure.bot.near, size: size)
            }
            // In a window not seen he is drawn once it is.
            if !visible {
                figure.drawn = false
            } else if moved || !figure.drawn {
                figure.drawn = figure.renderer.draw(figure.bot, relief: relief, screen: screen, scale: scale)
                if figure.drawn, let label = figure.label {
                    CATransaction.begin()
                    CATransaction.setDisableActions(true)
                    let top = figure.bot.foot.y + figure.bot.height * 1.02 + 9
                    label.position = CGPoint(x: CGFloat(figure.bot.foot.x), y: CGFloat(top))
                    CATransaction.commit()
                }
            }
        }
    }

    /// The snowman at a point, if any, and how near he is.
    func figure(at point: SIMD2<Float>) -> (id: String, near: Float)? {
        for (id, figure) in figures where figure.bot.contains(point) {
            return (id, figure.bot.near)
        }
        return nil
    }

    /// How hot this player's arm is from throwing: each throw adds one, and it cools to a fifth in about 2.5 seconds.
    private var heat: Float = 0
    private var heatAt: Double = 0

    /// Throws a snowball from in front of the screen at `point` in the picture, where it is `near`; it hits a
    /// snowman still there when it comes down, and a friend's tells the friend. The first throw after a rest goes
    /// where it is aimed; the faster they follow, the farther they stray: one a second, about a third of a snowman's
    /// height there, so that one aimed at his middle hits him about every other time; one every half second or
    /// faster, near three times that.
    func throwBall(at point: SIMD2<Float>, near: Float, size: Float, screenWidth: Float) {
        guard let simulation else { return }
        let start = SIMD2(screenWidth * 0.5 + (point.x - screenWidth * 0.5) * 0.35, -30)
        Self.throwsMade += 1
        let now = CACurrentMediaTime()
        heat *= exp(-Float(now - heatAt) / 1.5)
        heatAt = now
        let stray = min(heat, 3)
        heat += 1
        let height = size * 10
        let u = Float.random(in: Float.ulpOfOne..<1), v = Float.random(in: 0..<1)
        let spread = (-2 * log(u)).squareRoot() * stray
        let landing = point + SIMD2(cos(2 * .pi * v) * 0.3, sin(2 * .pi * v) * 0.25) * spread * height
        simulation.throwIntoPicture(from: start, to: landing, near: near, size: size) { [weak self, unowned simulation] landed in
            guard let self, let (id, figure) = figures.first(where: { $0.value.bot.contains(landed) && abs($0.value.bot.near - near) < 0.08 }) else { return false }
            // A hit on a friend counts only in a round still going, both standing; one that doesn't bursts on by.
            if id != Self.botID, !SnowballRoom.shared.hit(id) { return false }
            let now = CACurrentMediaTime()
            figure.bot.hit(at: landed, now: now)
            simulation.puff(at: landed, near: figure.bot.near, size: size * 1.4)
            if id == Self.botID {
                // He has no hearts: every hit moves him, until the last breaks him apart.
                botLives -= 1
                if botLives <= 0 {
                    figure.bot.knockOut()
                    botBackAt = now + 5
                } else {
                    figure.bot.moveAway()
                }
            } else {
                noteHearts(of: figure, id: id, now: now)
                moveIfOwed(figure)
            }
            return true
        }
    }

    /// Someone in the room hit someone. Hit by a friend, the friend's snowman throws at the screen; one friend hitting
    /// another, the one throws at the other across the picture. This player's own throws were seen as they went.
    private func thrown(from: String, at: String) {
        guard main, let me = SnowballRoom.shared.me, from != me else { return }
        if let figure = figures[from] {
            figure.bot.throwNow(at: at == me ? nil : at)
        } else if at == me, let simulation {
            simulation.throwAtScreen(from: SIMD2(simulation.width * Float.random(in: 0.3...0.7), simulation.height * 0.4), size: 6)
        }
    }
}
