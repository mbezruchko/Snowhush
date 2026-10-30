import MetalKit
import QuartzCore

struct Uniforms {
    var viewSize: SIMD2<Float>
    var scale: Float
    var time: Float
    var columnWidth: Float
    var columnCount: Int32 = 0
    var pileTop: Float = 0
    var occluderCount: Int32 = 0
    var origin: SIMD2<Float> = .zero
    var pileBottom: Float = 0
    var padding: Float = 0
    var light = SIMD4<Float>(1, 1, 1, 1)
    var shadowTint = SIMD4<Float>(0, 0, 0, 0)
    var rim = SIMD4<Float>(0, 0, 0, 0)
    /// The cursor's glow: where it is, how far it reaches and how bright it is.
    var lantern = SIMD4<Float>(0, 0, 0, 0)
    /// How far everything is jolted: x and y.
    var shake = SIMD4<Float>(0, 0, 0, 0)
    /// The pumpkins' candles: where each burns, how far its light reaches and how bright it is.
    var pumpkins = (SIMD4<Float>(0, 0, 0, 0), SIMD4<Float>(0, 0, 0, 0), SIMD4<Float>(0, 0, 0, 0))
}

struct PumpkinUniforms {
    var anchor: SIMD2<Float>
    var width: Float
    var seed: Float
}

struct SignUniforms {
    var anchor: SIMD2<Float>
    var size: SIMD2<Float>
    var angle: Float
    var ground: Float
}

enum RendererError: Error {
    case unavailable
}

final class Renderer: NSObject, MTKViewDelegate {
    var cursorLocation: () -> SIMD2<Float> = { SIMD2(-10_000, -10_000) }
    var windowSurfaces: (CFTimeInterval) -> WindowUpdate? = { _ in nil }
    /// Frames per second while the snow is being disturbed: the display's highest rate, up to 120 on ProMotion screens.
    var interactiveFrameRate = 60
    var budget = PowerBudget.full
    /// Where the Dock's snow is drawn; without one it stays in the main overlay.
    var dockOverlay: DockOverlay?
    var sceneLight: (CFTimeInterval) -> (light: SIMD3<Float>, shadowTint: SIMD4<Float>, rim: SIMD4<Float>, lantern: Float) = { _ in (SIMD3(repeating: 1), .zero, .zero, 0) }
    private var ambient: (light: SIMD3<Float>, shadowTint: SIMD4<Float>, rim: SIMD4<Float>, lantern: Float) = (SIMD3(repeating: 1), .zero, .zero, 0)
    private var lanternAt = SIMD2<Float>(-10_000, -10_000)

    private let simulation: SnowSimulation
    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let spritePipeline: MTLRenderPipelineState
    private let pilePipeline: MTLRenderPipelineState
    private let signPipeline: MTLRenderPipelineState
    private let pumpkinPipeline: MTLRenderPipelineState
    /// The pumpkin's shell and its face, drawn the first time it is needed.
    private var pumpkinPictures: (shell: MTLTexture, face: MTLTexture)?
    /// Each caption's sign, drawn the first time a ghost holds it up, and its size in points.
    private var signPictures: [Int: (texture: MTLTexture, size: SIMD2<Float>)] = [:]
    private let framesInFlight = 3
    private let semaphore: DispatchSemaphore
    private var spriteBuffers: [MTLBuffer] = []
    private var heightBuffers: [MTLBuffer] = []
    private var spriteCapacity = 0
    private var heightCapacity = 0
    private var slot = 0
    private var lastTime: CFTimeInterval = 0
    private var idleFrames = 0
    /// Whether the Dock's strip still shows snow.
    private var dockDrawn = false
    private let epoch = CACurrentMediaTime()

    init(device: MTLDevice, pixelFormat: MTLPixelFormat, simulation: SnowSimulation) throws {
        guard let queue = device.makeCommandQueue() else { throw RendererError.unavailable }
        let library = try device.makeLibrary(source: shaderSource, options: nil)

        func pipeline(_ vertex: String, _ fragment: String) throws -> MTLRenderPipelineState {
            let descriptor = MTLRenderPipelineDescriptor()
            descriptor.vertexFunction = library.makeFunction(name: vertex)
            descriptor.fragmentFunction = library.makeFunction(name: fragment)
            let attachment = descriptor.colorAttachments[0]!
            attachment.pixelFormat = pixelFormat
            attachment.isBlendingEnabled = true
            attachment.sourceRGBBlendFactor = .one
            attachment.sourceAlphaBlendFactor = .one
            attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
            attachment.destinationAlphaBlendFactor = .oneMinusSourceAlpha
            return try device.makeRenderPipelineState(descriptor: descriptor)
        }

        self.device = device
        self.queue = queue
        self.simulation = simulation
        self.semaphore = DispatchSemaphore(value: framesInFlight)
        self.spritePipeline = try pipeline("spriteVertex", "spriteFragment")
        self.pilePipeline = try pipeline("pileVertex", "pileFragment")
        self.signPipeline = try pipeline("signVertex", "signFragment")
        self.pumpkinPipeline = try pipeline("pumpkinVertex", "pumpkinFragment")
        super.init()
    }

    func resetClock() {
        lastTime = 0
    }

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    /// Snow is soft enough that one pixel per point is indistinguishable from Retina, at a quarter of the fill and memory.
    static func fitDrawable(of view: MTKView) {
        view.drawableSize = view.bounds.size
    }

    func draw(in view: MTKView) {
        let now = CACurrentMediaTime()
        let dt = lastTime > 0 ? Float(min(now - lastTime, 0.05)) : 1.0 / 60
        lastTime = now

        ambient = sceneLight(now)
        if let update = windowSurfaces(now) {
            simulation.updateWindows(update.surfaces, interval: update.interval, parked: update.parked, elsewhere: update.elsewhere, sliding: update.sliding)
            if update.slideVelocity != 0 { simulation.spaceSlid(velocity: update.slideVelocity) }
        }
        let cursor = cursorLocation()
        let changed = simulation.step(dt: dt, cursor: cursor)
        // The lantern follows the cursor even over snow that lies still.
        let lanternMoved = ambient.lantern > 0 && simd_distance(cursor, lanternAt) > 0.5
        lanternAt = cursor
        let fastest = budget == .full ? interactiveFrameRate : min(60, interactiveFrameRate)
        // 24 only where it divides the display's rate evenly; elsewhere it would judder.
        let calm = budget == .saver && interactiveFrameRate % 24 == 0 ? 24 : 30
        let frameRate = switch simulation.pace {
        case .calm: calm
        case .lively: min(60, fastest)
        case .interactive: fastest
        }
        let pace = lanternMoved ? max(frameRate, min(60, fastest)) : frameRate
        if view.preferredFramesPerSecond != pace { view.preferredFramesPerSecond = pace }
        idleFrames = changed || lanternMoved ? 0 : idleFrames + 1
        // A still pile only needs a slow refresh for its glints.
        guard changed || lanternMoved || ((simulation.hasSnow || !simulation.pumpkins.isEmpty) && idleFrames % 2 == 0) else { return }

        semaphore.wait()
        guard let pass = view.currentRenderPassDescriptor,
              let drawable = view.currentDrawable,
              let commandBuffer = queue.makeCommandBuffer() else {
            semaphore.signal()
            return
        }
        let scale = Float(view.drawableSize.width / max(view.bounds.width, 1))
        encode(into: commandBuffer, pass: pass, scale: scale, time: Float((now - epoch).truncatingRemainder(dividingBy: 1000)))
        commandBuffer.present(drawable)
        commandBuffer.addCompletedHandler { [semaphore] _ in semaphore.signal() }
        commandBuffer.commit()
    }

    func encode(into commandBuffer: MTLCommandBuffer, pass: MTLRenderPassDescriptor, scale: Float, time: Float) {
        reserve(sprites: simulation.spriteCount, columns: simulation.columnCount)
        guard spriteBuffers.count == framesInFlight, heightBuffers.count == framesInFlight,
              let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else { return }

        let index = slot % framesInFlight
        slot += 1
        let spriteBuffer = spriteBuffers[index]
        let heightBuffer = heightBuffers[index]
        let sprites = simulation.writeSprites(
            into: spriteBuffer.contents().bindMemory(to: Sprite.self, capacity: spriteCapacity),
            capacity: spriteCapacity
        )
        let piles = simulation.writeLayers(
            into: heightBuffer.contents().bindMemory(to: Float.self, capacity: heightCapacity),
            capacity: heightCapacity
        )

        var uniforms = Uniforms(
            viewSize: SIMD2(simulation.width, simulation.height),
            scale: scale,
            time: time,
            columnWidth: simulation.columnWidth,
            light: SIMD4(ambient.light, 1),
            shadowTint: ambient.shadowTint,
            rim: ambient.rim,
            lantern: SIMD4(lowHalf: lanternAt, highHalf: SIMD2(130, ambient.lantern)),
            shake: SIMD4(lowHalf: simulation.shake, highHalf: .zero)
        )
        let pumpkins = simulation.pumpkins
        let candles = pumpkins.prefix(3).map { pumpkin in
            let height = pumpkin.width * HalloweenPumpkin.aspect
            let flame = pumpkin.anchor + (HalloweenPumpkin.candle - SIMD2(0.5, 0)) * SIMD2(pumpkin.width, height)
            return SIMD4(lowHalf: flame, highHalf: SIMD2(pumpkin.width * 1.6, 0.4 + 0.6 * ambient.lantern))
        }
        if candles.count > 0 { uniforms.pumpkins.0 = candles[0] }
        if candles.count > 1 { uniforms.pumpkins.1 = candles[1] }
        if candles.count > 2 { uniforms.pumpkins.2 = candles[2] }
        let uniformsLength = MemoryLayout<Uniforms>.stride

        drawPumpkins(pumpkins, with: encoder, uniforms: uniforms)
        drawPiles(piles.filter { !$0.isDock }, with: encoder, uniforms: uniforms, heights: heightBuffer)
        drawSigns(simulation.signs, with: encoder, uniforms: uniforms)
        if sprites > 0 {
            encoder.setRenderPipelineState(spritePipeline)
            encoder.setVertexBuffer(spriteBuffer, offset: 0, index: 0)
            encoder.setVertexBytes(&uniforms, length: uniformsLength, index: 1)
            encoder.setFragmentBytes(&uniforms, length: uniformsLength, index: 1)
            encoder.drawPrimitives(type: .point, vertexStart: 0, vertexCount: sprites)
        }
        encoder.endEncoding()
        encodeDock(piles.filter(\.isDock), into: commandBuffer, uniforms: uniforms, heights: heightBuffer)
    }

    /// Draws the Dock's snow into its own strip below the Dock, and clears the strip once after the snow is gone.
    private func encodeDock(_ piles: [PileLayer], into commandBuffer: MTLCommandBuffer, uniforms: Uniforms, heights: MTLBuffer) {
        guard let dockOverlay, !piles.isEmpty || dockDrawn else { return }
        let top = piles.map { CGFloat($0.origin.y + $0.top + 2) }.max()
        guard let frame = dockOverlay.nextFrame(reaching: top),
              let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: frame.pass) else { return }
        var uniforms = uniforms
        uniforms.viewSize = SIMD2(simulation.width, Float(dockOverlay.height))
        drawPiles(piles, with: encoder, uniforms: uniforms, heights: heights)
        encoder.endEncoding()
        commandBuffer.present(frame.drawable)
        dockDrawn = !piles.isEmpty
    }

    private func drawPiles(_ piles: [PileLayer], with encoder: MTLRenderCommandEncoder, uniforms: Uniforms, heights: MTLBuffer) {
        guard !piles.isEmpty else { return }
        var uniforms = uniforms
        let uniformsLength = MemoryLayout<Uniforms>.stride
        encoder.setRenderPipelineState(pilePipeline)
        for pile in piles {
            uniforms.columnCount = Int32(pile.count)
            uniforms.pileTop = pile.top + 2
            uniforms.pileBottom = pile.bottom
            uniforms.origin = pile.origin
            uniforms.occluderCount = Int32(pile.occluders.count)
            var occluders = pile.occluders.isEmpty ? [SIMD4<Float>(repeating: 0)] : pile.occluders
            encoder.setVertexBytes(&uniforms, length: uniformsLength, index: 0)
            encoder.setFragmentBuffer(heights, offset: pile.offset * MemoryLayout<Float>.stride, index: 0)
            encoder.setFragmentBytes(&uniforms, length: uniformsLength, index: 1)
            encoder.setFragmentBytes(&occluders, length: occluders.count * MemoryLayout<SIMD4<Float>>.stride, index: 2)
            encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        }
    }

    private func drawSigns(_ signs: [SignSprite], with encoder: MTLRenderCommandEncoder, uniforms: Uniforms) {
        guard !signs.isEmpty else { return }
        var uniforms = uniforms
        encoder.setRenderPipelineState(signPipeline)
        for sign in signs {
            guard let picture = signPicture(sign.caption) else { continue }
            var shape = SignUniforms(anchor: sign.anchor, size: picture.size, angle: sign.angle, ground: sign.ground)
            encoder.setVertexBytes(&shape, length: MemoryLayout<SignUniforms>.stride, index: 0)
            encoder.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
            encoder.setFragmentBytes(&shape, length: MemoryLayout<SignUniforms>.stride, index: 0)
            encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
            encoder.setFragmentTexture(picture.texture, index: 0)
            encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        }
    }

    /// The pumpkins go down before the snow, so the drift they sit in covers their bottoms.
    private func drawPumpkins(_ pumpkins: [PumpkinSprite], with encoder: MTLRenderCommandEncoder, uniforms: Uniforms) {
        guard !pumpkins.isEmpty, let pictures = pumpkinPictures ?? makePumpkinPictures() else { return }
        var uniforms = uniforms
        encoder.setRenderPipelineState(pumpkinPipeline)
        encoder.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
        encoder.setFragmentTexture(pictures.shell, index: 0)
        encoder.setFragmentTexture(pictures.face, index: 1)
        for pumpkin in pumpkins {
            var shape = PumpkinUniforms(anchor: pumpkin.anchor, width: pumpkin.width, seed: pumpkin.seed)
            encoder.setVertexBytes(&shape, length: MemoryLayout<PumpkinUniforms>.stride, index: 0)
            encoder.setFragmentBytes(&shape, length: MemoryLayout<PumpkinUniforms>.stride, index: 0)
            encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        }
    }

    /// Drawn large and mipmapped: the pumpkins are shown at a fraction of that, and smaller ones would shimmer.
    private func makePumpkinPictures() -> (shell: MTLTexture, face: MTLTexture)? {
        func texture(_ part: HalloweenPumpkin.Part) -> MTLTexture? {
            guard let context = HalloweenPumpkin.draw(part, scale: 2), let data = context.data else { return nil }
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm, width: context.width, height: context.height, mipmapped: true)
            descriptor.usage = .shaderRead
            guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
            texture.replace(region: MTLRegionMake2D(0, 0, context.width, context.height), mipmapLevel: 0, withBytes: data, bytesPerRow: context.bytesPerRow)
            return texture
        }
        guard let shell = texture(.shell), let face = texture(.face),
              let commandBuffer = queue.makeCommandBuffer(), let blit = commandBuffer.makeBlitCommandEncoder() else { return nil }
        blit.generateMipmaps(for: shell)
        blit.generateMipmaps(for: face)
        blit.endEncoding()
        commandBuffer.commit()
        commandBuffer.waitUntilCompleted()
        pumpkinPictures = (shell, face)
        return pumpkinPictures
    }

    private func signPicture(_ caption: Int) -> (texture: MTLTexture, size: SIMD2<Float>)? {
        if let picture = signPictures[caption] { return picture }
        guard let context = HalloweenSign.draw(HalloweenSign.captions[caption]), let data = context.data else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm, width: context.width, height: context.height, mipmapped: false)
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        texture.replace(region: MTLRegionMake2D(0, 0, context.width, context.height), mipmapLevel: 0, withBytes: data, bytesPerRow: context.bytesPerRow)
        let picture = (texture: texture, size: SIMD2(Float(context.width), Float(context.height)) / Float(HalloweenSign.scale))
        signPictures[caption] = picture
        return picture
    }

    private func reserve(sprites: Int, columns: Int) {
        if sprites > spriteCapacity || spriteBuffers.isEmpty {
            spriteCapacity = max(1024, sprites * 3 / 2)
            spriteBuffers = makeBuffers(length: spriteCapacity * MemoryLayout<Sprite>.stride)
        }
        if columns > heightCapacity || heightBuffers.isEmpty {
            heightCapacity = max(256, columns)
            heightBuffers = makeBuffers(length: heightCapacity * MemoryLayout<Float>.stride)
        }
    }

    private func makeBuffers(length: Int) -> [MTLBuffer] {
        (0..<framesInFlight).compactMap { _ in
            device.makeBuffer(length: length, options: [.storageModeShared, .cpuCacheModeWriteCombined])
        }
    }
}
