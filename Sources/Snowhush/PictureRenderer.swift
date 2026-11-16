import Foundation
import Metal
import simd

/// Draws the snow lying in a wallpaper into a picture of the whole screen: its cover on the surfaces that hold it and
/// the caps along its ridges. It draws only when asked, when that snow has changed; what it draws is then shown as it
/// is, frame after frame, for nothing.
final class PictureRenderer {
    private struct Uniforms {
        var viewSize: SIMD2<Float>
        var scale: Float
        var level: Float
        var range: SIMD4<Float>
        var thickest: Float
    }

    private struct CapUniforms {
        var left: Float
        var columnWidth: Float
        var thickness: Float
        var near: Float
    }

    /// One ridge's cap: where its columns start in the buffer of them all.
    private struct Cap {
        let offset: Int
        let count: Int
        let left: Float
        let near: Float
    }

    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let liePipeline: MTLRenderPipelineState
    private let showPipeline: MTLRenderPipelineState
    private let capPipeline: MTLRenderPipelineState
    /// Where the first pass leaves how the snow lies, for the second to show: half the picture's size across.
    private var lie: MTLTexture?
    private let noise: MTLTexture

    private var relief: MTLTexture?
    private var light: MTLTexture?
    private var dabs: MTLTexture?
    private var dabsVersion = -1
    private var columns: MTLBuffer?
    private var caps: [Cap] = []
    private var range = SIMD4<Float>(0, 1, Wallpaper.ground, 0)
    private var columnWidth: Float = 2

    /// A cap's full thickness on the nearest ridge once all the snow has fallen, in points.
    private static let capDepth: Float = 9

    private static var built: [UInt64: (lie: MTLRenderPipelineState, show: MTLRenderPipelineState, cap: MTLRenderPipelineState)] = [:]
    /// How thick the snow lies on the nearest level ground once all of it has fallen, in points.
    private static let depth: Float = 5

    init(device: MTLDevice) throws {
        guard let queue = device.makeCommandQueue(), let noise = Self.makeNoise(device: device) else { throw RendererError.unavailable }
        self.device = device
        self.queue = queue
        self.noise = noise
        if let pipelines = Self.built[device.registryID] {
            (liePipeline, showPipeline, capPipeline) = pipelines
        } else {
            let library = try device.makeLibrary(source: pictureShaderSource, options: nil)
            func pipeline(_ vertex: String, _ fragment: String, format: MTLPixelFormat = .bgra8Unorm, blended: Bool = true) throws -> MTLRenderPipelineState {
                let descriptor = MTLRenderPipelineDescriptor()
                descriptor.vertexFunction = library.makeFunction(name: vertex)
                descriptor.fragmentFunction = library.makeFunction(name: fragment)
                let attachment = descriptor.colorAttachments[0]!
                attachment.pixelFormat = format
                attachment.isBlendingEnabled = blended
                attachment.sourceRGBBlendFactor = .one
                attachment.sourceAlphaBlendFactor = .one
                attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
                attachment.destinationAlphaBlendFactor = .oneMinusSourceAlpha
                return try device.makeRenderPipelineState(descriptor: descriptor)
            }
            liePipeline = try pipeline("pictureCoverVertex", "pictureLieFragment", format: .rgba8Unorm, blended: false)
            showPipeline = try pipeline("pictureCoverVertex", "pictureShowFragment")
            capPipeline = try pipeline("pictureCapVertex", "pictureCapFragment")
            Self.built[device.registryID] = (liePipeline, showPipeline, capPipeline)
        }
    }

    /// Takes in what was found in a wallpaper; nil for none.
    func load(_ scene: WallpaperScene?, columnWidth: Float) {
        self.columnWidth = columnWidth
        dabs = nil
        dabsVersion = -1
        guard let scene else {
            relief = nil
            light = nil
            columns = nil
            caps = []
            return
        }
        let terrain = scene.terrain
        range = SIMD4(terrain.far, terrain.close, Wallpaper.ground, 0)
        light = texture(.rgba8Unorm, width: scene.light.width, height: scene.light.height, bytes: scene.light.pixels, perPixel: 4)
        // The four planes of the relief side by side in one texture: lightness, tops, hold, near.
        let relief = scene.relief
        var planes = Data(count: relief.width * relief.height * 4)
        planes.withUnsafeMutableBytes { (packed: UnsafeMutableRawBufferPointer) in
            for (k, plane) in [relief.lightness, relief.tops, relief.hold, relief.near].enumerated() {
                plane.withUnsafeBytes { (source: UnsafeRawBufferPointer) in
                    for i in 0..<min(source.count, relief.width * relief.height) { packed[4 * i + k] = source[i] }
                }
            }
        }
        self.relief = texture(.rgba8Unorm, width: relief.width, height: relief.height, bytes: planes, perPixel: 4, mipmapped: true)
        layCaps(along: scene.ridges)
    }

    /// Whether a wallpaper's snow can be drawn.
    var loaded: Bool { relief != nil && light != nil }

    private func texture(_ format: MTLPixelFormat, width: Int, height: Int, bytes: Data, perPixel: Int, mipmapped: Bool = false) -> MTLTexture? {
        guard width > 0, height > 0, bytes.count >= width * height * perPixel else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: format, width: width, height: height, mipmapped: mipmapped)
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        bytes.withUnsafeBytes {
            texture.replace(region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: $0.baseAddress!, bytesPerRow: width * perPixel)
        }
        if mipmapped, texture.mipmapLevelCount > 1, let commandBuffer = queue.makeCommandBuffer(), let blit = commandBuffer.makeBlitCommandEncoder() {
            blit.generateMipmaps(for: texture)
            blit.endEncoding()
            commandBuffer.commit()
        }
        return texture
    }

    /// Works out each ridge's cap: thinner where the ridge climbs steeply, uneven along it as drifted snow is, and
    /// tapering away to its ends.
    private func layCaps(along ridges: [WallpaperRidge]) {
        var values: [Float] = []
        caps = []
        for ridge in ridges where ridge.tops.count > 4 {
            let tops = ridge.tops
            let count = tops.count
            // The same ridge always gets the same cap, however often its wallpaper is read again.
            let seed = ridge.left * 12.9898 + Float(count) * 78.233
            let phases = (2 * Float.pi * Self.fraction(sin(seed) * 43758.547), 2 * Float.pi * Self.fraction(sin(seed + 1.7) * 43758.547))
            caps.append(Cap(offset: values.count * MemoryLayout<Float>.stride, count: count, left: ridge.left, near: min(max(ridge.nearness, 0), 1)))
            for k in 0..<count {
                let a = max(k - 3, 0), b = min(k + 3, count - 1)
                let slope = abs(tops[b] - tops[a]) / (Float(b - a) * columnWidth)
                let lie = min(max(1 - slope / 2.2, 0.3), 1)
                let x = Float(k) * columnWidth
                let uneven = 0.85 + 0.2 * sin(x * 0.045 + phases.0) + 0.1 * sin(x * 0.13 + phases.1)
                let end = min(Float(min(k, count - 1 - k)) / 10, 1)
                let taper = end * end * (3 - 2 * end)
                let low = max(k - 5, 0), high = min(k + 5, count - 1)
                let smooth = tops[low...high].reduce(0, +) / Float(high - low + 1)
                values += [tops[k], smooth, lie * uneven * taper, 0]
            }
        }
        columns = values.isEmpty ? nil : device.makeBuffer(bytes: values, length: values.count * MemoryLayout<Float>.stride, options: .storageModeShared)
    }

    private static func fraction(_ x: Float) -> Float { x - x.rounded(.down) }

    /// Draws the snow into `target`, the whole screen at `scale` pixels a point, and calls `done` on the main thread once
    /// it is there. `level` is how much has fallen everywhere, `dabs` what the flakes seen landing have left.
    @discardableResult
    func draw(level: Float, dabs bytes: [UInt8], dabsSize: (width: Int, height: Int), dabsVersion version: Int,
              into target: MTLTexture, screen: SIMD2<Float>, then done: @escaping () -> Void) -> Bool {
        guard let relief, let light, dabsSize.width > 0, bytes.count >= dabsSize.width * dabsSize.height,
              let commandBuffer = queue.makeCommandBuffer() else { return false }
        if dabs == nil || dabs?.width != dabsSize.width || dabs?.height != dabsSize.height {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .r8Unorm, width: dabsSize.width, height: dabsSize.height, mipmapped: false)
            descriptor.usage = .shaderRead
            dabs = device.makeTexture(descriptor: descriptor)
            dabsVersion = -1
        }
        guard let dabs else { return false }
        if dabsVersion != version {
            dabsVersion = version
            dabs.replace(region: MTLRegionMake2D(0, 0, dabsSize.width, dabsSize.height), mipmapLevel: 0, withBytes: bytes, bytesPerRow: dabsSize.width)
        }
        let half = (width: max(target.width / 2, 1), height: max(target.height / 2, 1))
        if lie == nil || lie?.width != half.width || lie?.height != half.height {
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: half.width, height: half.height, mipmapped: false)
            descriptor.usage = [.renderTarget, .shaderRead]
            descriptor.storageMode = .private
            lie = device.makeTexture(descriptor: descriptor)
        }
        guard let lie else { return false }
        func pass(into texture: MTLTexture) -> MTLRenderPassDescriptor {
            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = texture
            pass.colorAttachments[0].loadAction = .clear
            pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 0)
            pass.colorAttachments[0].storeAction = .store
            return pass
        }
        var uniforms = Uniforms(viewSize: screen, scale: Float(target.width) / max(screen.x, 1), level: level, range: range,
                                thickest: max(Self.depth * pow(level, 0.7), 0.01))
        let length = MemoryLayout<Uniforms>.stride
        // How the snow lies, at half the size: nothing it is worked out from is finer.
        guard let lying = commandBuffer.makeRenderCommandEncoder(descriptor: pass(into: lie)) else { return false }
        lying.setRenderPipelineState(liePipeline)
        lying.setVertexBytes(&uniforms, length: length, index: 0)
        lying.setFragmentBytes(&uniforms, length: length, index: 0)
        lying.setFragmentTexture(dabs, index: 0)
        lying.setFragmentTexture(relief, index: 1)
        lying.setFragmentTexture(noise, index: 3)
        lying.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        lying.endEncoding()
        guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass(into: target)) else { return false }
        encoder.setRenderPipelineState(showPipeline)
        encoder.setVertexBytes(&uniforms, length: length, index: 0)
        encoder.setFragmentBytes(&uniforms, length: length, index: 0)
        encoder.setFragmentTexture(lie, index: 0)
        encoder.setFragmentTexture(relief, index: 1)
        encoder.setFragmentTexture(light, index: 2)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        if let columns, !caps.isEmpty {
            encoder.setRenderPipelineState(capPipeline)
            for cap in caps {
                // The farther off the ridge, the thinner the same snow looks on it.
                var uniforms = CapUniforms(left: cap.left, columnWidth: columnWidth,
                                           thickness: Self.capDepth * pow(level, 0.8) * (0.35 + 0.65 * cap.near), near: cap.near)
                encoder.setVertexBytes(&uniforms, length: MemoryLayout<CapUniforms>.stride, index: 1)
                encoder.setFragmentBytes(&uniforms, length: MemoryLayout<CapUniforms>.stride, index: 1)
                encoder.setVertexBuffer(columns, offset: cap.offset, index: 2)
                encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 2 * cap.count)
            }
        }
        encoder.endEncoding()
        commandBuffer.addCompletedHandler { _ in DispatchQueue.main.async(execute: done) }
        commandBuffer.commit()
        return true
    }

    /// A gradient noise that tiles, 32 cells across 256 pixels: unlike noise of values at a grid's corners it has no
    /// square blobs for a threshold to cut out. Always the same, so the snow's edge doesn't move between drawings.
    private static func makeNoise(device: MTLDevice) -> MTLTexture? {
        let size = 256, cells = 32
        var state: UInt64 = 0x9E3779B97F4A7C15
        func next() -> Float {
            state = state &* 6364136223846793005 &+ 1442695040888963407
            return Float(state >> 40) / Float(1 << 24)
        }
        let slopes = (0..<(cells * cells)).map { _ -> SIMD2<Float> in
            let angle = next() * 2 * .pi
            return SIMD2(cos(angle), sin(angle))
        }
        func slope(_ x: Int, _ y: Int) -> SIMD2<Float> { slopes[((y + cells) % cells) * cells + (x + cells) % cells] }
        var pixels = [UInt8](repeating: 0, count: size * size)
        let step = Float(cells) / Float(size)
        for py in 0..<size {
            for px in 0..<size {
                let p = SIMD2(Float(px) + 0.5, Float(py) + 0.5) * step
                let i = SIMD2(p.x.rounded(.down), p.y.rounded(.down)), f = p - i
                let x = Int(i.x), y = Int(i.y)
                let s = f * f * f * (f * (f * 6 - 15) + 10)
                let a = simd_dot(slope(x, y), f), b = simd_dot(slope(x + 1, y), f - SIMD2(1, 0))
                let c = simd_dot(slope(x, y + 1), f - SIMD2(0, 1)), d = simd_dot(slope(x + 1, y + 1), f - SIMD2(1, 1))
                let top = a + (b - a) * s.x, bottom = c + (d - c) * s.x
                pixels[py * size + px] = UInt8(min(max(0.5 + 0.75 * (top + (bottom - top) * s.y), 0), 1) * 255)
            }
        }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .r8Unorm, width: size, height: size, mipmapped: false)
        descriptor.usage = .shaderRead
        let texture = device.makeTexture(descriptor: descriptor)
        texture?.replace(region: MTLRegionMake2D(0, 0, size, size), mipmapLevel: 0, withBytes: pixels, bytesPerRow: size)
        return texture
    }
}
