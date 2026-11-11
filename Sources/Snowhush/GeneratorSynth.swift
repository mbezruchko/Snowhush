import Foundation

/// A small petrol generator running outside, made up as it plays: the cylinder firing into its muffler, the
/// alternator's hum, the valves' tick and the engine's wash, heard through a window. Nothing in `render` allocates,
/// locks or retains, so it runs on the audio thread.
struct GeneratorSynth {
    /// Firing strokes a second: a four-stroke single at 3000 rpm, turning a two-pole alternator at 50 Hz.
    static let firingRate: Float = 25
    /// Loudness before the volume: about -18 dBFS RMS, with room for the firing peaks.
    private static let level: Float = 0.55

    private let sampleRate: Float
    private var random: UInt32 = 0x9E37_79B9

    /// Through the firing cycle, 0 to 1; the crank turns twice in it.
    private var phase: Float = 0
    /// Through the alternator's cycle, twice a firing cycle.
    private var humPhase: Float = 0
    /// The governor hunting around its speed, -1 to 1, easing towards a target picked every second or two.
    private var hunt: Float = 0
    private var huntTarget: Float = 0
    private var huntLeft = 0
    private let huntEase: Float

    private var bang: Float = 0
    private let bangDecay: Float
    private var thump: Float = 0
    private let thumpDecay: Float
    private var tick: Float = 0
    private let tickDecay: Float
    private var ticked = false

    private var muffler: Biquad
    private var mufflerOvertone: Biquad
    private var tailpipe: Biquad
    private var valves: Biquad
    private var wash: Biquad
    private var window: Biquad
    private var windowRolloff: Biquad
    private var gain: Float = 0
    private let gainEase: Float

    init(sampleRate: Double) {
        let rate = Float(sampleRate)
        self.sampleRate = rate
        let decay = { (seconds: Float) in exp(-1 / (seconds * rate)) }
        bangDecay = decay(0.004)
        thumpDecay = decay(0.018)
        tickDecay = decay(0.0015)
        huntEase = 1 - exp(-2 * .pi * 0.35 / rate)
        gainEase = 1 - exp(-1 / (0.25 * rate))
        muffler = .bandpass(95, q: 3, sampleRate: rate)
        mufflerOvertone = .bandpass(210, q: 4, sampleRate: rate)
        tailpipe = .lowpass(600, q: 0.7, sampleRate: rate)
        valves = .bandpass(2600, q: 1.5, sampleRate: rate)
        wash = .bandpass(900, q: 0.7, sampleRate: rate)
        window = .lowpass(1600, q: 0.6, sampleRate: rate)
        windowRolloff = .lowpass(1600, q: 0.6, sampleRate: rate)
    }

    /// Fills `samples` with the next `count` samples, easing the loudness towards `target` (0 to 1) so that starting
    /// and stopping never click.
    mutating func render(into samples: UnsafeMutablePointer<Float>, count: Int, target: Float) {
        for index in 0..<count {
            gain += (target - gain) * gainEase
            samples[index] = next() * gain * Self.level
        }
    }

    private mutating func next() -> Float {
        if huntLeft <= 0 {
            huntTarget = noise()
            huntLeft = Int(sampleRate * (0.8 + 1.7 * unit()))
        }
        huntLeft -= 1
        hunt += (huntTarget - hunt) * huntEase

        let step = Self.firingRate * (1 + 0.015 * hunt) / sampleRate
        phase += step
        humPhase += 2 * step
        if humPhase >= 1 { humPhase -= 1 }
        if phase >= 1 {
            phase -= 1
            ticked = false
            // No two strokes burn quite alike.
            let strength = 0.75 + 0.25 * unit()
            bang = strength
            thump = strength
        }
        // A valve opens once a crank turn, halfway between strokes.
        if !ticked, phase >= 0.5 {
            ticked = true
            tick = 0.6 + 0.4 * unit()
        }
        bang *= bangDecay
        thump *= thumpDecay
        tick *= tickDecay

        let hiss = noise()
        let stroke = bang * (0.4 + 0.6 * hiss)
        let exhaust = muffler.process(stroke) * 3.2 + mufflerOvertone.process(stroke) * 1.6 + tailpipe.process(stroke) * 0.5
        let body = sin(2 * .pi * 60 * Float(phase) / Self.firingRate) * thump * 0.25
        let hum = 0.5 * sin(2 * .pi * humPhase) + 0.25 * sin(4 * .pi * humPhase) + 0.12 * sin(6 * .pi * humPhase)
        let clatter = valves.process(tick * hiss) * 0.5
        let rumble = wash.process(hiss) * (0.5 + 0.5 * thump) * 0.08

        let mixed = exhaust + body + hum * 0.06 + clatter + rumble
        let heard = windowRolloff.process(window.process(mixed))
        return tanh(heard * 1.4) / 1.4
    }

    /// Uniform in -1 to 1.
    private mutating func noise() -> Float { unit() * 2 - 1 }

    /// Uniform in 0 to 1, from xorshift32.
    private mutating func unit() -> Float {
        random ^= random << 13
        random ^= random >> 17
        random ^= random << 5
        return Float(random >> 8) / Float(1 << 24)
    }
}

/// A second-order filter from the Audio EQ Cookbook, in transposed direct form II.
struct Biquad {
    private let b0, b1, b2, a1, a2: Float
    private var z1: Float = 0
    private var z2: Float = 0

    private init(b0: Float, b1: Float, b2: Float, a0: Float, a1: Float, a2: Float) {
        self.b0 = b0 / a0
        self.b1 = b1 / a0
        self.b2 = b2 / a0
        self.a1 = a1 / a0
        self.a2 = a2 / a0
    }

    /// Unity gain at `frequency`.
    static func bandpass(_ frequency: Float, q: Float, sampleRate: Float) -> Biquad {
        let w = 2 * .pi * frequency / sampleRate
        let alpha = sin(w) / (2 * q)
        return Biquad(b0: alpha, b1: 0, b2: -alpha, a0: 1 + alpha, a1: -2 * cos(w), a2: 1 - alpha)
    }

    static func lowpass(_ frequency: Float, q: Float, sampleRate: Float) -> Biquad {
        let w = 2 * .pi * frequency / sampleRate
        let alpha = sin(w) / (2 * q)
        let cosine = cos(w)
        return Biquad(b0: (1 - cosine) / 2, b1: 1 - cosine, b2: (1 - cosine) / 2, a0: 1 + alpha, a1: -2 * cosine, a2: 1 - alpha)
    }

    mutating func process(_ x: Float) -> Float {
        let y = b0 * x + z1
        z1 = b1 * x - a1 * y + z2
        z2 = b2 * x - a2 * y
        return y
    }
}
