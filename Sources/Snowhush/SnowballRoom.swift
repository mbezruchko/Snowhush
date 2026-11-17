import AppKit
import CSnowhushNet

/// The room of a snowball fight between friends, if this player is in one: who else is there, and what they throw.
/// The network itself is net/, in Rust; this asks it what happened a few times a second while there is a room.
final class SnowballRoom {
    static let shared = SnowballRoom()

    struct Player: Equatable {
        let id: String
        let name: String
    }

    enum State: Equatable {
        case none
        case opening
        case joining
        /// In a room, as `me`.
        case open(me: String)
        case failed(String)
    }

    private(set) var state = State.none
    /// Everyone else in the room.
    private(set) var players: [Player] = []
    /// The invite to the room, when hosting it.
    private(set) var invite: String?
    /// How many snowballs a snowman takes before he falls apart.
    static let startingLives = 5
    /// What is left of each player's snowman this round, by id, this player's among them. Every player hears of every
    /// hit, so all count the same.
    private(set) var lives: [String: Int] = [:]
    /// Rounds each player has won, by id.
    private(set) var wins: [String: Int] = [:]
    /// The one left standing, until the next round starts.
    private(set) var winner: String?
    private var nextRound: Timer?

    func lives(of player: String) -> Int { lives[player] ?? Self.startingLives }
    func wins(of player: String) -> Int { wins[player] ?? 0 }
    func isOut(_ player: String) -> Bool { lives(of: player) <= 0 }

    /// A player's name, this player's as "You".
    func name(of player: String) -> String {
        player == me ? tr("fight.you") : players.first { $0.id == player }?.name ?? ""
    }

    /// Who is in the room changed, or the room opened, closed or failed.
    static let changed = Notification.Name("SnowballRoomChanged")
    /// Someone hit someone: `from` and `at`, players' ids.
    static let thrown = Notification.Name("SnowballRoomThrown")

    var me: String? {
        if case let .open(me) = state { return me }
        return nil
    }

    private var timer: Timer?

    /// What friends see this player called: the first of the Mac user's names.
    static var name: String {
        NSFullUserName().split(separator: " ").first.map(String.init) ?? "Snowman"
    }

    /// What snowhush.app/relay.json says about playing online: the relay to go through, "default" for iroh's public
    /// ones, and whether to play online at all. The relay can so be changed for everyone, or play stopped, without a
    /// new release.
    private struct Settings: Decodable {
        let relay: String?
        let online: Bool?
    }

    private static let settings = URL(string: "https://snowhush.app/relay.json")!

    /// The relay to go through and whether playing online is on: a relay of one's own if set, else what the site
    /// says, else what it last said, else iroh's public relays. Asked only on opening or joining a room, three seconds
    /// at most.
    private static func withSettings(_ then: @escaping (_ relay: String?, _ online: Bool) -> Void) {
        if let own = UserDefaults.standard.string(forKey: "fightRelay"), !own.isEmpty {
            then(own, true)
            return
        }
        let request = URLRequest(url: settings, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 3)
        let session = URLSession(configuration: .ephemeral)
        session.dataTask(with: request) { data, response, _ in
            let known = UserDefaults.standard
            if let data, (response as? HTTPURLResponse)?.statusCode == 200, let said = try? JSONDecoder().decode(Settings.self, from: data) {
                let relay = said.relay.flatMap { URL(string: $0)?.scheme == "https" ? $0 : nil }
                known.set(relay, forKey: "fightRelayKnown")
                known.set(said.online ?? true, forKey: "fightOnlineKnown")
            }
            let relay = known.string(forKey: "fightRelayKnown")
            let online = known.object(forKey: "fightOnlineKnown") as? Bool ?? true
            DispatchQueue.main.async { then(relay, online) }
        }.resume()
        session.finishTasksAndInvalidate()
    }

    /// The link that brings a friend into the room.
    static func link(for invite: String) -> String { "snowhush://join/\(invite)" }

    /// Opens a room; its invite link goes to the clipboard as soon as it is there.
    func host() {
        reset()
        state = .opening
        post()
        Self.withSettings { [weak self] relay, online in
            guard let self, state == .opening else { return }
            guard online else { return unavailable() }
            sn_host(Self.name, relay)
            listen()
        }
    }

    /// Joins the room of an invite, given as a link or as it is.
    func join(_ invite: String) {
        reset()
        state = .joining
        post()
        let invite = invite.trimmingCharacters(in: .whitespacesAndNewlines)
        Self.withSettings { [weak self] relay, online in
            guard let self, state == .joining else { return }
            guard online else { return unavailable() }
            sn_join(invite, Self.name, relay)
            listen()
        }
    }

    private func unavailable() {
        state = .failed(tr("fight.unavailable"))
        post()
    }

    /// Says this player hit `player`.
    /// Says this player hit `player`; it counts only while both are in the round. Returns whether it counted.
    @discardableResult
    func hit(_ player: String) -> Bool {
        guard let me, counts(from: me, at: player) else { return false }
        sn_throw(player)
        // The room doesn't tell a player of its own throws.
        land(at: player)
        return true
    }

    /// Whether a hit from one player at another counts: in a round still going, both still standing.
    func counts(from: String, at: String) -> Bool {
        winner == nil && !isOut(from) && !isOut(at)
    }

    private func land(at player: String) {
        lives[player] = lives(of: player) - 1
        settleRound()
        post()
    }

    /// Everyone's snowman whole again.
    private func startRound() {
        nextRound?.invalidate()
        nextRound = nil
        lives = [:]
        winner = nil
    }

    /// With one left standing of two or more, he wins the round, and another starts a few seconds later.
    private func settleRound() {
        guard winner == nil, let me else { return }
        let everyone = [me] + players.map(\.id)
        guard everyone.count >= 2 else { return }
        let standing = everyone.filter { !isOut($0) }
        if standing.isEmpty { return startRound() }
        guard standing.count == 1 else { return }
        winner = standing[0]
        wins[standing[0], default: 0] += 1
        let timer = Timer(timeInterval: 8, repeats: false) { [weak self] _ in
            self?.startRound()
            self?.post()
        }
        RunLoop.main.add(timer, forMode: .common)
        nextRound = timer
    }

    func leave() {
        sn_leave()
        reset()
        post()
    }

    /// Leaves as the app quits, the others told before it is gone.
    func leaveNow() {
        guard state != .none else { return }
        sn_leave_now()
        reset()
    }

    /// Whether some text holds an invite.
    static func holdsInvite(_ text: String) -> Bool {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return text.hasPrefix("snowhush://join/") && text.count > 60
    }

    private func reset() {
        timer?.invalidate()
        timer = nil
        state = .none
        players = []
        invite = nil
        startRound()
        wins = [:]
    }

    private func listen() {
        let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in self?.poll() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func poll() {
        while let text = sn_next() {
            defer { sn_free(text) }
            guard let data = String(cString: text).data(using: .utf8),
                  let event = try? JSONSerialization.jsonObject(with: data) as? [String: String] else { continue }
            take(event)
        }
    }

    private func take(_ event: [String: String]) {
        switch event["type"] {
        case "room":
            guard let me = event["me"], let ticket = event["ticket"] else { return }
            state = .open(me: me)
            invite = ticket
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(Self.link(for: ticket), forType: .string)
            post()
        case "joined":
            guard let me = event["me"] else { return }
            state = .open(me: me)
            post()
        case "player":
            guard let id = event["id"], let name = event["name"], !players.contains(where: { $0.id == id }) else { return }
            players.append(Player(id: id, name: name))
            // Someone new comes into a fresh round, everyone whole: all start it as they hear of him.
            startRound()
            post()
        case "left":
            players.removeAll { $0.id == event["id"] }
            if let id = event["id"] {
                lives[id] = nil
                wins[id] = nil
            }
            settleRound()
            post()
        case "throw":
            guard let from = event["from"], let at = event["at"], from != me, counts(from: from, at: at) else { return }
            NotificationCenter.default.post(name: Self.thrown, object: self, userInfo: ["from": from, "at": at])
            land(at: at)
        case "closed":
            sn_leave()
            reset()
            state = .failed(event["reason"] == "full" ? tr("fight.full") : tr("fight.closed"))
            post()
        case "error":
            sn_leave()
            reset()
            state = .failed(tr("fight.failed"))
            post()
        default:
            break
        }
    }

    private func post() {
        NotificationCenter.default.post(name: Self.changed, object: self)
    }
}
