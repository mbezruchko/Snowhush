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
    var sceneLight: (CFTimeInterval) -> (light: SIMD3<Float>, shadowTint: SIMD4<Float>) = { _ in (SIMD3(repeating: 1), .zero) }
    private var ambient: (light: SIMD3<Float>, shadowTint: SIMD4<Float>) = (SIMD3(repeating: 1), .zero)

    private let simulation: SnowSimulation
    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let spritePipeline: MTLRenderPipelineState
    private let pilePipeline: MTLRenderPipelineState
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
        }
        let changed = simulation.step(dt: dt, cursor: cursorLocation())
        let fastest = budget == .full ? interactiveFrameRate : min(60, interactiveFrameRate)
        // 24 only where it divides the display's rate evenly; elsewhere it would judder.
        let calm = budget == .saver && interactiveFrameRate % 24 == 0 ? 24 : 30
        let frameRate = switch simulation.pace {
        case .calm: calm
        case .lively: min(60, fastest)
        case .interactive: fastest
        }
        if view.preferredFramesPerSecond != frameRate { view.preferredFramesPerSecond = frameRate }
        idleFrames = changed ? 0 : idleFrames + 1
        // A still pile only needs a slow refresh for its glints.
        guard changed || (simulation.hasSnow && idleFrames % 2 == 0) else { return }

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
            shadowTint: ambient.shadowTint
        )
        let uniformsLength = MemoryLayout<Uniforms>.stride

        drawPiles(piles.filter { !$0.isDock }, with: encoder, uniforms: uniforms, heights: heightBuffer)
        if sprites > 0 {
            encoder.setRenderPipelineState(spritePipeline)
            encoder.setVertexBuffer(spriteBuffer, offset: 0, index: 0)
            encoder.setVertexBytes(&uniforms, length: uniformsLength, index: 1)
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
