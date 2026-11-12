import Metal
import XCTest
@testable import Snowhush

final class WeatherVisualTests: XCTestCase {
    func testMoonHasALocalizedDiscAtDifferentAspectRatios() throws {
        let renderer = try WeatherRenderFixture()
        for (width, height) in [(640, 400), (1600, 900), (1000, 1000)] {
            let frame = try renderer.draw(width: width, height: height, layers: .zero, environment: SIMD4(0, 0, 1, 1))
            let bright = frame.points(above: 140)
            XCTAssertFalse(bright.isEmpty, "A glow alone is not a lunar disc")
            let minX = try XCTUnwrap(bright.map(\.x).min()), maxX = try XCTUnwrap(bright.map(\.x).max())
            let minY = try XCTUnwrap(bright.map(\.y).min()), maxY = try XCTUnwrap(bright.map(\.y).max())
            XCTAssertLessThanOrEqual(maxX - minX, 60, "The moon must not grow to cover the desktop")
            XCTAssertEqual(Double(maxX - minX), Double(maxY - minY), accuracy: 2, "Wide displays must not stretch the moon")
            XCTAssertEqual(Double(minX + maxX) / 2, Double(width) * 0.83, accuracy: 2)
            XCTAssertLessThan(bright.count, width * height / 100)
            let radius = min(max(Double(min(width, height)) * 0.028, 18), 28)
            let x = Int(Double(width) * 0.83 + radius * 4), y = Int(max(64, Double(height) * 0.12))
            XCTAssertLessThan(frame.alpha(x: x, y: y), 10, "The halo must stay close to the moon")
        }
    }

    func testLightningHasABoltAndLeavesTheLowerDesktopClear() throws {
        let renderer = try WeatherRenderFixture()
        var timing = WeatherLightning()
        timing.step(dt: 1.25, enabled: true, calm: false)
        let frame = try renderer.draw(layers: SIMD4(0, 0, 0, 1), environment: SIMD4(0, 0, 1, 0), flash: timing.flash)
        let bright = frame.points(above: 110)
        XCTAssertGreaterThan(bright.count, 30, "The strike needs a visible core, not just a diffuse stain")
        XCTAssertLessThan(bright.count, frame.width * frame.height / 100)
        XCTAssertTrue(bright.allSatisfy { $0.y < frame.height / 2 })
        for y in (frame.height / 2)..<frame.height {
            for x in 0..<frame.width { XCTAssertEqual(frame.alpha(x: x, y: y), 0) }
        }
    }

    func testLightningIsTransparentBetweenStrikesAndWithReducedMotion() throws {
        let renderer = try WeatherRenderFixture()
        let idle = try renderer.draw(layers: SIMD4(0, 0, 0, 1), environment: SIMD4(0, 0, 1, 0))
        XCTAssertTrue(idle.points(above: 0).isEmpty, "No continuous oval wash when the storm is idle")
        var effects = WeatherEffects.none
        effects[.lightning] = .always
        var sky = WeatherAtmosphere(scene: .resolve(reading: nil, effects: effects))
        sky.step(dt: 1.25, calm: true)
        let calm = try renderer.draw(layers: sky.layers, environment: sky.environment, flash: sky.lightning.flash)
        XCTAssertTrue(calm.points(above: 0).isEmpty)
        let off = try renderer.draw(layers: .zero, environment: SIMD4(1, 0, 1, 0))
        XCTAssertTrue(off.points(above: 0).isEmpty, "A night tint must not draw a moon when its switch is off")
    }
}

private final class WeatherRenderFixture {
    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let pipeline: MTLRenderPipelineState

    init() throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw XCTSkip("Metal is unavailable on this runner") }
        self.device = device
        queue = try XCTUnwrap(device.makeCommandQueue())
        let library = try device.makeLibrary(source: shaderSource + "\n" + weatherShaderSource, options: nil)
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "weatherVertex")
        descriptor.fragmentFunction = library.makeFunction(name: "weatherFragment")
        descriptor.colorAttachments[0].pixelFormat = .rgba8Unorm
        pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
    }

    struct Frame {
        let width: Int, height: Int
        let bytes: [UInt8]
        func alpha(x: Int, y: Int) -> UInt8 { bytes[(y * width + x) * 4 + 3] }
        func points(above threshold: UInt8) -> [(x: Int, y: Int)] {
            var points: [(x: Int, y: Int)] = []
            for y in 0..<height {
                for x in 0..<width where alpha(x: x, y: y) > threshold { points.append((x, y)) }
            }
            return points
        }
    }

    func draw(width: Int = 640, height: Int = 400, layers: SIMD4<Float>, environment: SIMD4<Float>, flash: SIMD4<Float> = .zero) throws -> Frame {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: width, height: height, mipmapped: false)
        descriptor.storageMode = .shared
        descriptor.usage = [.renderTarget]
        let texture = try XCTUnwrap(device.makeTexture(descriptor: descriptor))
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].storeAction = .store
        pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 0)
        let buffer = try XCTUnwrap(queue.makeCommandBuffer())
        let encoder = try XCTUnwrap(buffer.makeRenderCommandEncoder(descriptor: pass))
        var uniforms = WeatherUniforms(layers: layers, environment: environment, viewport: SIMD4(Float(width), Float(height), 6.15, 1), lightning: flash)
        encoder.setRenderPipelineState(pipeline)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<WeatherUniforms>.stride, index: 0)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
        encoder.endEncoding()
        buffer.commit()
        buffer.waitUntilCompleted()
        XCTAssertNil(buffer.error)
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        texture.getBytes(&bytes, bytesPerRow: width * 4, from: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0)
        return Frame(width: width, height: height, bytes: bytes)
    }
}
