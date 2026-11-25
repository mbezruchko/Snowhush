import AppKit

/// A short, original noise texture. Audio is opt-in; the trackpad response remains subtle.
final class SnowFeedback {
    static let shared = SnowFeedback()
    static var soundEnabled: Bool {
        get { UserDefaults.standard.bool(forKey: "snowInteractionSound") }
        set { UserDefaults.standard.set(newValue, forKey: "snowInteractionSound") }
    }
    private let sounds: [NSSound]
    private var lastPlayed: TimeInterval = -10
    private var nextSound = 0

    private init() {
        sounds = (0..<3).compactMap { NSSound(data: Self.crunch(seed: UInt32($0 + 1))) }
    }

    func play(strength: CGFloat) {
        let now = ProcessInfo.processInfo.systemUptime
        guard now - lastPlayed > 0.14 else { return }
        lastPlayed = now
        NSHapticFeedbackManager.defaultPerformer.perform(strength > 0.5 ? .levelChange : .alignment, performanceTime: .now)
        guard Self.soundEnabled, !sounds.isEmpty else { return }
        let sound = sounds[nextSound % sounds.count]
        nextSound += 1
        sound.stop()
        sound.volume = Float(0.12 + min(1, max(0, strength)) * 0.12)
        sound.play()
    }

    private static func crunch(seed: UInt32) -> Data {
        let rate = 22_050, frames = 1_984
        var data = Data()
        func word(_ value: UInt16) { var little = value.littleEndian; withUnsafeBytes(of: &little) { data.append(contentsOf: $0) } }
        func long(_ value: UInt32) { var little = value.littleEndian; withUnsafeBytes(of: &little) { data.append(contentsOf: $0) } }
        data.append(contentsOf: "RIFF".utf8); long(UInt32(36 + frames * 2))
        data.append(contentsOf: "WAVEfmt ".utf8); long(16); word(1); word(1)
        long(UInt32(rate)); long(UInt32(rate * 2)); word(2); word(16)
        data.append(contentsOf: "data".utf8); long(UInt32(frames * 2))
        var random = seed, low: Double = 0
        for i in 0..<frames {
            random = random &* 1_664_525 &+ 1_013_904_223
            let noise = Double(random) / Double(UInt32.max) * 2 - 1
            low += (noise - low) * 0.22
            let t = Double(i) / Double(frames)
            let envelope = min(1, t * 35) * pow(1 - t, 2.5)
            let grain = 0.65 + 0.35 * sin(t * 85 + Double(seed)) * sin(t * 85 + Double(seed))
            let sample = Int16(max(-32767, min(32767, low * envelope * grain * 20_000)))
            word(UInt16(bitPattern: sample))
        }
        return data
    }
}
