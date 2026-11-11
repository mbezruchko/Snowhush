import Metal
import simd

struct TreeUniforms {
    /// The drawn region's bottom left and size, in the screen's points.
    var origin: SIMD2<Float>
    var size: SIMD2<Float>
    var time: Float
    var bulbCount: Float = 0
    var shake = SIMD2<Float>(repeating: 0)
    var light = SIMD4<Float>(1, 1, 1, 1)
    var shadowTint = SIMD4<Float>(repeating: 0)
}

/// One bough, bauble, bulb, star or present, as the tree's shaders read it.
struct TreePiece {
    var a: SIMD4<Float>
    var b: SIMD4<Float>
    var c: SIMD4<Float>
    var d = SIMD4<Float>(repeating: 0)
}

struct TreeBulb {
    var place: SIMD4<Float>
    var color: SIMD4<Float>
}

/// Draws the New Year's tree: its boughs as strips bent along them, then what hangs on it and lies under it.
final class TreeRenderer {
    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let boughPipeline: MTLRenderPipelineState
    private let decorPipeline: MTLRenderPipelineState
    private let sparkPipeline: MTLRenderPipelineState
    private var sparkBuffers: [MTLBuffer] = []
    private var sparkSlot = 0
    private var needles: MTLTexture?
    private var buffers: [MTLBuffer] = []
    private var slot = 0
    private static let bufferLength = 64 * 1024
    private static let boughSegments = 10

    init(device: MTLDevice, queue: MTLCommandQueue, pixelFormat: MTLPixelFormat) throws {
        self.device = device
        self.queue = queue
        let library = try device.makeLibrary(source: treeShaderSource, options: nil)
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
        boughPipeline = try pipeline("boughVertex", "boughFragment")
        decorPipeline = try pipeline("decorVertex", "decorFragment")
        sparkPipeline = try pipeline("sparkVertex", "sparkFragment")
        buffers = (0..<3).compactMap { _ in device.makeBuffer(length: Self.bufferLength, options: .storageModeShared) }
    }

    func encode(_ tree: ChristmasTree, with encoder: MTLRenderCommandEncoder, uniforms: TreeUniforms) {
        guard buffers.count == 3, let needles = needles ?? makeNeedles() else { return }
        let buffer = buffers[slot % 3]
        slot += 1

        var bulbs: [TreeBulb] = []
        var back: [TreePiece] = []
        var front: [TreePiece] = []
        for bulb in tree.bulbs {
            let place = tree.bulbPosition(bulb)
            let near = tree.boughs[bulb.bough].depth > 0
            bulbs.append(TreeBulb(place: SIMD4(place, 1.7, near ? 1 : 0), color: SIMD4(bulb.color, bulb.brightness)))
            let piece = TreePiece(a: SIMD4(place, 1.7, near ? 1 : 0), b: SIMD4(bulb.color, bulb.brightness), c: SIMD4(0, 0, 0, 1))
            if near { front.append(piece) } else { back.append(piece) }
        }
        var boughs: [TreePiece] = []
        for bough in tree.boughs {
            if bough.trunk || bough.core {
                let top = (bough.height + bough.reach.y) / ChristmasTree.height
                boughs.append(TreePiece(
                    a: SIMD4(tree.base + SIMD2(0, bough.height), bough.reach),
                    b: SIMD4(tree.sway * top * top, 0, bough.width, -1),
                    c: SIMD4(0, bough.seed, 0, bough.trunk ? 1 : 2),
                    d: SIMD4(bough.up, 0, 0)
                ))
                continue
            }
            let flat = min(max((bough.up.y - 0.55) / 0.3, 0), 1)
            boughs.append(TreePiece(
                a: SIMD4(tree.root(of: bough), bough.reach),
                b: SIMD4(bough.offset, bough.width, bough.depth),
                c: SIMD4(bough.load, bough.seed, bough.variant, 0),
                d: SIMD4(bough.up, 3.8 * bough.load.squareRoot() * flat, 0)
            ))
        }
        var decor = front
        for bauble in tree.baubles {
            decor.insert(TreePiece(
                a: SIMD4(tree.ball(of: bauble), bauble.radius, 0), b: SIMD4(bauble.color, bauble.seed), c: SIMD4(bauble.pivot, 0, 0)
            ), at: 0)
        }
        if let leader = tree.boughs.firstIndex(where: { $0.capacity == 0 && !$0.trunk && !$0.core }) {
            let tip = tree.point(leader, 1)
            let lean = tree.point(leader, 1) - tree.point(leader, 0.8)
            let angle = atan2(lean.x, lean.y)
            decor.append(TreePiece(a: SIMD4(tip + simd_normalize(lean) * 6, 8.5, angle), b: SIMD4(tree.starGlow, 0, 0, 0), c: SIMD4(0, 0, 0, 3)))
        }
        for present in tree.presents {
            decor.append(TreePiece(
                a: SIMD4(tree.base.x + present.offset + present.jiggle, present.bottom, present.width, present.height),
                b: SIMD4(present.lid, present.snow / present.width, present.style, present.offset * 0.013),
                c: SIMD4(0, 0, 0, 2)
            ))
            guard present.arm > 0.01 else { continue }
            // The arm rises out of the box and bends over toward the hand.
            let shoulder = tree.shoulder(of: present)
            let out = present.arm * present.arm * (3 - 2 * present.arm)
            let hand = shoulder + (present.hand - shoulder) * out
            let length = simd_distance(hand, shoulder)
            let bend = shoulder + SIMD2(0, length * 0.55)
            decor.append(TreePiece(
                a: SIMD4(shoulder, hand),
                b: SIMD4(bend, present.grip, present.wave),
                c: SIMD4(present.bottom + present.height, 0, 0, 4)
            ))
        }

        let stride = MemoryLayout<TreePiece>.stride
        let boughOffset = 0
        let backOffset = align(boughOffset + boughs.count * stride)
        let decorOffset = align(backOffset + back.count * stride)
        let bulbOffset = align(decorOffset + decor.count * stride)
        guard bulbOffset + max(bulbs.count, 1) * MemoryLayout<TreeBulb>.stride <= Self.bufferLength else { return }
        let base = buffer.contents()
        boughs.withUnsafeBytes { base.advanced(by: boughOffset).copyMemory(from: $0.baseAddress!, byteCount: $0.count) }
        if !back.isEmpty { back.withUnsafeBytes { base.advanced(by: backOffset).copyMemory(from: $0.baseAddress!, byteCount: $0.count) } }
        decor.withUnsafeBytes { base.advanced(by: decorOffset).copyMemory(from: $0.baseAddress!, byteCount: $0.count) }
        if !bulbs.isEmpty { bulbs.withUnsafeBytes { base.advanced(by: bulbOffset).copyMemory(from: $0.baseAddress!, byteCount: $0.count) } }

        var uniforms = uniforms
        uniforms.bulbCount = Float(bulbs.count)
        let length = MemoryLayout<TreeUniforms>.stride
        encoder.setVertexBytes(&uniforms, length: length, index: 1)
        encoder.setFragmentBytes(&uniforms, length: length, index: 1)
        encoder.setFragmentBuffer(buffer, offset: bulbOffset, index: 2)

        encoder.setRenderPipelineState(decorPipeline)
        if !back.isEmpty {
            encoder.setVertexBuffer(buffer, offset: backOffset, index: 0)
            encoder.setFragmentBuffer(buffer, offset: backOffset, index: 0)
            encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: back.count)
        }
        encoder.setRenderPipelineState(boughPipeline)
        encoder.setVertexBuffer(buffer, offset: boughOffset, index: 0)
        encoder.setFragmentBuffer(buffer, offset: boughOffset, index: 0)
        encoder.setFragmentTexture(needles, index: 0)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: (Self.boughSegments + 1) * 2, instanceCount: boughs.count)
        encoder.setRenderPipelineState(decorPipeline)
        encoder.setVertexBuffer(buffer, offset: decorOffset, index: 0)
        encoder.setFragmentBuffer(buffer, offset: decorOffset, index: 0)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: decor.count)
    }

    /// The fireworks' sparks, each a glowing streak along its flight.
    func encode(_ fireworks: Fireworks, with encoder: MTLRenderCommandEncoder, uniforms: TreeUniforms) {
        let sparks = fireworks.sparks
        guard !sparks.isEmpty else { return }
        let length = sparks.count * MemoryLayout<TreePiece>.stride
        if sparkBuffers.first.map({ $0.length < length }) ?? true {
            sparkBuffers = (0..<3).compactMap { _ in device.makeBuffer(length: max(length * 2, 64 * 1024), options: .storageModeShared) }
        }
        guard sparkBuffers.count == 3 else { return }
        let buffer = sparkBuffers[sparkSlot % 3]
        sparkSlot += 1
        let pieces = buffer.contents().bindMemory(to: TreePiece.self, capacity: sparks.count)
        for (k, spark) in sparks.enumerated() {
            pieces[k] = TreePiece(a: SIMD4(spark.position, spark.velocity), b: SIMD4(spark.color, spark.brightness), c: SIMD4(spark.size, spark.flash ? 1 : 0, 0, 5))
        }
        var uniforms = uniforms
        encoder.setRenderPipelineState(sparkPipeline)
        encoder.setVertexBuffer(buffer, offset: 0, index: 0)
        encoder.setVertexBytes(&uniforms, length: MemoryLayout<TreeUniforms>.stride, index: 1)
        encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: sparks.count)
    }

    private func align(_ offset: Int) -> Int {
        (offset + 255) / 256 * 256
    }

    /// Drawn large and mipmapped: the boughs are shown at a fraction of that, and smaller ones would shimmer.
    private func makeNeedles() -> MTLTexture? {
        let descriptor = MTLTextureDescriptor()
        descriptor.textureType = .type2DArray
        descriptor.pixelFormat = .bgra8Unorm
        descriptor.width = TreePictures.size.width
        descriptor.height = TreePictures.size.height
        descriptor.arrayLength = TreePictures.variants
        descriptor.mipmapLevelCount = Int(log2(Double(max(descriptor.width, descriptor.height)))) + 1
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        for variant in 0..<TreePictures.variants {
            guard let context = TreePictures.bough(variant), let data = context.data else { return nil }
            texture.replace(region: MTLRegionMake2D(0, 0, context.width, context.height), mipmapLevel: 0, slice: variant,
                            withBytes: data, bytesPerRow: context.bytesPerRow, bytesPerImage: context.bytesPerRow * context.height)
        }
        guard let commandBuffer = queue.makeCommandBuffer(), let blit = commandBuffer.makeBlitCommandEncoder() else { return nil }
        blit.generateMipmaps(for: texture)
        blit.endEncoding()
        commandBuffer.commit()
        commandBuffer.waitUntilCompleted()
        needles = texture
        return texture
    }
}

private extension SIMD4 where Scalar == Float {
    init(_ xy: SIMD2<Float>, _ z: Float, _ w: Float) {
        self.init(xy.x, xy.y, z, w)
    }

    init(_ xy: SIMD2<Float>, _ zw: SIMD2<Float>) {
        self.init(xy.x, xy.y, zw.x, zw.y)
    }
}
