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

    /// A relay of one's own, if set; else iroh's public ones, which are meant for trying things out.
    private static var relay: String? { UserDefaults.standard.string(forKey: "fightRelay") }

    /// The link that brings a friend into the room.
    static func link(for invite: String) -> String { "snowhush://join/\(invite)" }

    /// Opens a room; its invite link goes to the clipboard as soon as it is there.
    func host() {
        reset()
        state = .opening
        sn_host(Self.name, Self.relay)
        listen()
        post()
    }

    /// Joins the room of an invite, given as a link or as it is.
    func join(_ invite: String) {
        reset()
        state = .joining
        sn_join(invite.trimmingCharacters(in: .whitespacesAndNewlines), Self.name, Self.relay)
        listen()
        post()
    }

    /// Says this player hit `player`.
    func hit(_ player: String) {
        guard me != nil else { return }
        sn_throw(player)
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
            post()
        case "left":
            players.removeAll { $0.id == event["id"] }
            post()
        case "throw":
            guard let from = event["from"], let at = event["at"] else { return }
            NotificationCenter.default.post(name: Self.thrown, object: self, userInfo: ["from": from, "at": at])
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
