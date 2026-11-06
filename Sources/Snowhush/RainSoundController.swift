import AVFAudio
import Foundation

/// Ambient sound for Rain mode: a looping rain bed with thunder now and then.
///
/// The files live in `Resources/Sounds/` and are copied into the app by `build.sh`. One combined
/// rain-and-thunder file is enough: it is simply looped. Later, a `rain.*` bed plus any number of
/// `thunder-1.*`-style one-shots lets thunder be timed and switched on its own. Until there is at
/// least one audio file Rain mode is simply silent; nothing here is needed to build.
///
/// Every setting change comes through `update()`, which is idempotent: it works out whether sound
/// should be heard and how loud, and only touches the players when that answer changes.
final class RainSoundController {
    /// What falls. Only rain is heard.
    var mode: Mode = .snow
    /// The Rain Sounds menu toggle.
    var enabled = true
    /// The Thunder menu toggle.
    var thunderEnabled = true
    /// Master volume from the menu slider, 0...1.
    var volume: Float = 0.5
    /// Intensity sets how hard the rain sounds; Off is silent.
    var intensity: Snowfall = .steady
    /// False while asleep, locked, switched away, or under a full-screen app.
    var audible = true
    /// In Low Power Mode the rain is a little quieter.
    var powerBudget: PowerBudget = .full

    /// Whether separate thunder one-shots were found. When they weren't, the loop carries its own
    /// thunder and the menu has nothing extra to switch off.
    var hasThunderOneShots: Bool { !files.thunder.isEmpty }

    /// Made once and kept: pausing and resuming keeps the rain bed's place in its loop.
    private var rain: AVAudioPlayer?
    private var requestedLevel: Float = -1
    private var pauseWork: DispatchWorkItem?
    /// One-shot thunder players, held until they have finished.
    private var thunderPlayers: [AVAudioPlayer] = []
    private var thunderWork: DispatchWorkItem?
    private let files = SoundFiles()

    private static let fadeIn: TimeInterval = 1.2
    private static let fadeOut: TimeInterval = 0.8
    private static let adjust: TimeInterval = 0.3

    /// Recomputes what should be heard. Safe to call on every setting change.
    func update() {
        let wanted = mode == .rain && enabled && audible && intensity != .off
        let level = wanted ? Self.level(of: intensity, volume: volume, budget: powerBudget) : 0

        if wanted {
            pauseWork?.cancel()
            pauseWork = nil
            let created = makeRainIfNeeded()
            if let rain, !rain.isPlaying { rain.play() }
            setRain(level, fade: created ? Self.fadeIn : Self.adjust)
            scheduleThunder()
        } else {
            setRain(0, fade: Self.fadeOut)
            pauseRain()
            cancelThunder()
            hushThunder()
        }
    }

    // MARK: - Rain bed

    /// Returns whether a player was just made, so the first start can fade in slowly.
    private func makeRainIfNeeded() -> Bool {
        guard rain == nil, let url = files.loop else { return false }
        do {
            let player = try AVAudioPlayer(contentsOf: url)
            player.numberOfLoops = -1
            player.volume = 0
            guard player.prepareToPlay(), player.play() else { return false }
            rain = player
            requestedLevel = 0
            return true
        } catch {
            NSLog("Snowhush: could not load the rain sound: \(error)")
            return false
        }
    }

    private func setRain(_ level: Float, fade: TimeInterval) {
        guard abs(level - requestedLevel) > 0.001 else { return }
        requestedLevel = level
        rain?.setVolume(level, fadeDuration: fade)
    }

    /// A muted bed is paused rather than thrown away, so switching modes or waking the Mac
    /// resumes the same point in the loop instead of starting the file over.
    private func pauseRain() {
        guard rain?.isPlaying == true else { return }
        pauseWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.rain?.pause()
            self?.pauseWork = nil
        }
        pauseWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.fadeOut, execute: work)
    }

    // MARK: - Thunder

    private func scheduleThunder() {
        guard thunderEnabled, rain != nil, !files.thunder.isEmpty, thunderWork == nil else { return }
        let work = DispatchWorkItem { [weak self] in
            self?.thunderWork = nil
            self?.strike()
            self?.scheduleThunder()
        }
        thunderWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.thunderDelay(for: intensity), execute: work)
    }

    private func cancelThunder() {
        thunderWork?.cancel()
        thunderWork = nil
    }

    /// Thunder is less often in a light shower and more often in a downpour.
    private static func thunderDelay(for intensity: Snowfall) -> TimeInterval {
        let base = Double.random(in: 20...75)
        switch intensity {
        case .light: return base * 1.6
        case .heavy: return base * 0.7
        default: return base
        }
    }

    private func strike() {
        guard mode == .rain, enabled, audible, intensity != .off, thunderEnabled,
              let url = files.thunder.randomElement() else { return }
        do {
            let player = try AVAudioPlayer(contentsOf: url)
            player.volume = Self.level(of: intensity, volume: volume, budget: powerBudget) * Float.random(in: 0.6...1)
            guard player.prepareToPlay(), player.play() else { return }
            thunderPlayers.append(player)
            // One-shots are short; hold the player until a moment after it ends.
            let lifetime = max(1, player.duration + 0.5)
            DispatchQueue.main.asyncAfter(deadline: .now() + lifetime) { [weak self] in
                self?.thunderPlayers.removeAll { $0 === player }
            }
        } catch {
            NSLog("Snowhush: could not load a thunder sound: \(error)")
        }
    }

    /// Fades out claps already under way when the sound is switched off or the Mac rests.
    private func hushThunder() {
        let players = thunderPlayers
        thunderPlayers.removeAll()
        guard !players.isEmpty else { return }
        for player in players { player.setVolume(0, fadeDuration: 0.25) }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            for player in players { player.stop() }
        }
    }

    // MARK: - Level

    private static func level(of intensity: Snowfall, volume: Float, budget: PowerBudget) -> Float {
        let intensityScale: Float
        switch intensity {
        case .off: intensityScale = 0
        case .light: intensityScale = 0.45
        case .steady: intensityScale = 0.7
        case .heavy: intensityScale = 1
        case .variable: intensityScale = 0.8
        }
        let budgetScale: Float = budget == .saver ? 0.75 : 1
        return min(max(volume, 0), 1) * intensityScale * budgetScale
    }

    // MARK: - Sound files

    /// Finds the sounds in the app bundle, or in the package while running straight from Xcode.
    private struct SoundFiles {
        /// The bed that loops. It may already carry thunder of its own.
        let loop: URL?
        /// Separate one-shots, played now and then over the bed. May be empty.
        let thunder: [URL]

        private static let extensions: Set<String> = ["wav", "aiff", "aif", "m4a", "mp3", "caf", "aac"]

        init() {
            let found = Self.audioFiles()
            let oneShots = found.filter { Self.name(of: $0).hasPrefix("thunder") }
            let beds = found.filter { !Self.name(of: $0).hasPrefix("thunder") }
            let bed: URL?
            if found.count == 1 {
                // One file alone is the bed whatever it happens to be called.
                bed = found.first
            } else {
                bed = beds.first(where: { Self.name(of: $0) == "rain" }) ?? beds.first
            }
            loop = bed
            if let bed = bed {
                thunder = oneShots.filter { $0 != bed }
            } else {
                thunder = oneShots
            }
            if loop == nil {
                NSLog("Snowhush: no rain sound in Resources/Sounds — Rain mode stays silent until one is added.")
            }
        }

        private static func name(of url: URL) -> String {
            return url.deletingPathExtension().lastPathComponent.lowercased()
        }

        private static func audioFiles() -> [URL] {
            guard let directory = soundsDirectory() else { return [] }
            let contents = (try? FileManager.default.contentsOfDirectory(
                at: directory, includingPropertiesForKeys: nil, options: .skipsHiddenFiles)) ?? []
            return contents
                .filter { extensions.contains($0.pathExtension.lowercased()) }
                .sorted { $0.lastPathComponent < $1.lastPathComponent }
        }

        private static func soundsDirectory() -> URL? {
            let manager = FileManager.default
            var isDirectory = ObjCBool(false)
            if let bundled = Bundle.main.resourceURL?.appendingPathComponent("Sounds"),
               manager.fileExists(atPath: bundled.path, isDirectory: &isDirectory), isDirectory.boolValue {
                return bundled
            }
            // Xcode and `swift run` don't assemble the .app bundle that build.sh makes.
            let source = URL(fileURLWithPath: #filePath)
                .deletingLastPathComponent()   // Sources/Snowhush
                .deletingLastPathComponent()   // Sources
                .deletingLastPathComponent()   // package root
                .appendingPathComponent("Resources/Sounds")
            if manager.fileExists(atPath: source.path, isDirectory: &isDirectory), isDirectory.boolValue {
                return source
            }
            return nil
        }
    }
}
