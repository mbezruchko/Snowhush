import AVFoundation

/// The generator heard outside while the snow falls: played only while wanted and while the snow is, eased in and
/// out, and the audio engine stopped once it has faded so that silence costs nothing.
final class GeneratorSound {
    /// Wanted from the menu.
    var enabled = false {
        didSet { update() }
    }
    /// Loudness, 0 to 1.
    var volume = 0.5 {
        didSet { update() }
    }
    /// The snow is on screen: false while the screens sleep or lock.
    var active = true {
        didSet { if active != oldValue { update() } }
    }

    private let engine = AVAudioEngine()
    private var source: AVAudioSourceNode?
    /// Read by the audio thread each buffer; a lone aligned Float is written whole.
    private let target = UnsafeMutablePointer<Float>.allocate(capacity: 1)
    private var stopping: DispatchWorkItem?
    private var observer: NSObjectProtocol?

    init() {
        target.initialize(to: 0)
        // A new output device stops the engine; it is rebuilt for the device's rate if still playing.
        observer = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
            guard let self, source != nil else { return }
            teardown()
            update()
        }
    }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        teardown()
        target.deallocate()
    }

    private var playing: Bool { enabled && active && volume > 0 }

    private func update() {
        // Squared, so the slider's lower half stays usable for a quiet background.
        target.pointee = playing ? Float(volume * volume) : 0
        if playing {
            stopping?.cancel()
            stopping = nil
            start()
        } else if source != nil, stopping == nil {
            let stop = DispatchWorkItem { [weak self] in
                guard let self else { return }
                stopping = nil
                if !playing { teardown() }
            }
            stopping = stop
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5, execute: stop)
        }
    }

    private func start() {
        if source == nil {
            let rate = engine.outputNode.outputFormat(forBus: 0).sampleRate
            guard rate > 0, let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1) else { return }
            var synth = GeneratorSynth(sampleRate: rate)
            let target = target
            let node = AVAudioSourceNode(format: format) { _, _, frames, buffers in
                let list = UnsafeMutableAudioBufferListPointer(buffers)
                guard let samples = list.first?.mData?.assumingMemoryBound(to: Float.self) else { return noErr }
                synth.render(into: samples, count: Int(frames), target: target.pointee)
                return noErr
            }
            engine.attach(node)
            engine.connect(node, to: engine.mainMixerNode, format: format)
            source = node
        }
        guard !engine.isRunning else { return }
        do {
            try engine.start()
        } catch {
            NSLog("Snowhush: generator sound failed to start: \(error)")
            teardown()
        }
    }

    private func teardown() {
        engine.stop()
        if let source {
            engine.disconnectNodeOutput(source)
            engine.detach(source)
        }
        source = nil
    }
}
