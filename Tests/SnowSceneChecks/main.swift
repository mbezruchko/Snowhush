import AppKit
import MetalKit

func check(_ condition: @autoclosure () -> Bool, _ label: String) {
    precondition(condition(), label)
}
let application = NSApplication.shared
for rate in [60, 120] {
    check(PowerBudget.battery.frameRate(for: .lively, displayRate: rate) == 30, "Battery background must stay at 30 fps")
    check(PowerBudget.battery.frameRate(for: .interactive, displayRate: rate) == 60, "Battery interaction stays responsive")
    check(PowerBudget.saver.frameRate(for: .interactive, displayRate: rate) <= 30, "Saver caps interaction")
    check(rate % PowerBudget.saver.frameRate(for: .calm, displayRate: rate) == 0, "Saver cadence must divide refresh rate")
    check(PowerBudget.full.frameRate(for: .interactive, displayRate: rate) == rate, "Plugged-in interaction keeps display rate")
}
check(PowerBudget.saver.particleDensity < PowerBudget.battery.particleDensity, "Saver reduces particle work")
check(PowerBudget.battery.windowPollInterval > PowerBudget.full.windowPollInterval, "Battery reduces idle window queries")
let primary = SnowSimulation(width: 1920, height: 1080, intensity: 1)
let secondary = SnowSimulation(width: 1280, height: 800, intensity: 1)
primary.thaws = false
let surfaces: [WindowSurface] = (0..<8).map { index in
    let x = CGFloat((index % 4) * 450)
    let y = CGFloat((index / 4) * 350 + 100)
    let frame = CGRect(x: x, y: y, width: 400, height: 220)
    return WindowSurface(id: CGWindowID(index + 1), frame: frame, blockers: [], cover: [], cornerRadius: 12, cornerExponent: 2.5, isDock: false, sweeps: false)
}
primary.updateWindows(surfaces, interval: 0.1)
let absent = SIMD2<Float>(-10000, -10000)
for _ in 0..<1200 { _ = primary.step(dt: 1.0 / 60, cursor: absent) }
var timings: [Double] = []
for frame in 0..<600 {
    let before = ProcessInfo.processInfo.systemUptime
    _ = primary.step(dt: 1.0 / 60, cursor: SIMD2(Float(frame % 1920), 16))
    _ = secondary.step(dt: 1.0 / 60, cursor: absent)
    timings.append((ProcessInfo.processInfo.systemUptime - before) * 1000)
}
func validate(_ simulation: SnowSimulation) {
    let count = simulation.columnCount
    var memory = [Float](repeating: 0, count: max(1, count))
    let piles = memory.withUnsafeMutableBufferPointer { simulation.writeLayers(into: $0.baseAddress!, capacity: count) }
    check(memory.allSatisfy(\.isFinite), "All surface rows must remain finite")
    check(piles.allSatisfy { $0.offset + $0.count * SnowLayer.rows <= count }, "Six-row pile layout must fit its buffer")
}
validate(primary); validate(secondary)
if let borrowed = primary.lend(1, shift: SIMD2(-100, 0), within: CGRect(x: 0, y: 0, width: 1280, height: 800)) {
    secondary.setBorrowed([borrowed]); validate(secondary)
} else { preconditionFailure("Warm snow fixture must lend a window to the adjacent display") }
if let carried = primary.takeWindow(1) {
    secondary.adoptWindow(1, carried, shift: SIMD2(-100, 0))
} else { preconditionFailure("Expected window snow for transfer test") }
for size: SIMD2<Float> in [SIMD2(3840, 2160), SIMD2(1280, 800), SIMD2(1920, 1080)] {
    primary.resize(width: size.x, height: size.y)
    _ = primary.step(dt: 1.0 / 60, cursor: absent); validate(primary)
}
primary.updateWindows([], interval: 0.1)
for _ in 0..<120 { _ = primary.step(dt: 1.0 / 60, cursor: absent) }
validate(primary); validate(secondary)
let sorted = timings.sorted()
print(String(format: "Two simulated displays, blizzard, eight windows: mean %.3f ms/step, p95 %.3f ms/step", timings.reduce(0, +) / Double(timings.count), sorted[Int(Double(sorted.count) * 0.95)]))

// Compile every production pipeline, then draw the changed pile shader into a real Metal texture.
guard let device = MTLCreateSystemDefaultDevice() else { fatalError("Metal device required for shader verification") }
let renderer = try Renderer(device: device, pixelFormat: .bgra8Unorm, simulation: primary)
renderer.resetClock()
let library = try device.makeLibrary(source: shaderSource, options: nil)
let descriptor = MTLRenderPipelineDescriptor()
descriptor.vertexFunction = library.makeFunction(name: "pileVertex")
descriptor.fragmentFunction = library.makeFunction(name: "pileFragment")
descriptor.colorAttachments[0].pixelFormat = .rgba8Unorm
descriptor.colorAttachments[0].isBlendingEnabled = true
descriptor.colorAttachments[0].sourceRGBBlendFactor = .one
descriptor.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
let pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
let width = 640, height = 240
let textureDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: width, height: height, mipmapped: false)
textureDescriptor.usage = [.renderTarget]
textureDescriptor.storageMode = .shared
let texture = device.makeTexture(descriptor: textureDescriptor)!
let queue = device.makeCommandQueue()!
let command = queue.makeCommandBuffer()!
let pass = MTLRenderPassDescriptor()
pass.colorAttachments[0].texture = texture
pass.colorAttachments[0].loadAction = .clear
pass.colorAttachments[0].storeAction = .store
pass.colorAttachments[0].clearColor = MTLClearColor(red: 0.14, green: 0.2, blue: 0.26, alpha: 1)
let encoder = command.makeRenderCommandEncoder(descriptor: pass)!
encoder.setRenderPipelineState(pipeline)
let fixture = SnowLayer(origin: .zero, columns: 280, columnWidth: 2, depth: 60, opensAtEdges: true, phases: (0, 0))
let profile: [Float] = (0..<280).map { i in
    let x = Float(i) * 2
    let first: Float = 22 * exp(-pow((x - 160) / 80, 2))
    let second: Float = 15 * exp(-pow((x - 400) / 100, 2))
    let groove: Float = 15 * exp(-pow((x - 280) / 15, 2))
    return 16 + first + second - groove
}
fixture.replace(heights: profile)
for i in 120..<155 { fixture.compact(i, pressure: 1) }
fixture.refreshTop()
var heights = [Float](repeating: 0, count: fixture.count * SnowLayer.rows)
heights.withUnsafeMutableBufferPointer { fixture.writeSmoothed(into: $0.baseAddress!) }
let buffer = heights.withUnsafeBytes { device.makeBuffer(bytes: $0.baseAddress!, length: $0.count)! }
var uniforms = Uniforms(viewSize: SIMD2(Float(width), Float(height)), scale: 1, time: 0, columnWidth: 2)
uniforms.columnCount = Int32(fixture.count); uniforms.pileTop = fixture.top + 2; uniforms.origin = SIMD2(40, 100)
var blocker = SIMD4<Float>.zero
encoder.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
encoder.setFragmentBuffer(buffer, offset: 0, index: 0)
encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
encoder.setFragmentBytes(&blocker, length: MemoryLayout<SIMD4<Float>>.stride, index: 2)
encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
encoder.endEncoding(); command.commit(); command.waitUntilCompleted()
check(command.status == .completed, "Offscreen Metal rendering must complete")
var pixels = [UInt8](repeating: 0, count: width * height * 4)
texture.getBytes(&pixels, bytesPerRow: width * 4, from: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0)
check(stride(from: 0, to: pixels.count, by: 4).contains { pixels[$0] > 200 }, "Shader must actually draw snow")
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32)!
pixels.withUnsafeBytes { bitmap.bitmapData!.update(from: $0.baseAddress!.assumingMemoryBound(to: UInt8.self), count: pixels.count) }
let destination = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "/tmp/snowhush-snow-preview.png"
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: destination))
print("PASS: blizzard, two-display transfer, resizing, window closure, finite GPU buffers, Metal pipelines, offscreen snow rendering")
print("Preview: \(destination)")

// Compare steady background work at each actual power policy, including CPU geometry export.
// This measures simulation CPU wall time, not GPU energy, whole-process CPU or battery lifetime.
for budget: PowerBudget in [.full, .battery, .saver] {
    let scene = SnowSimulation(width: 1920, height: 1080, intensity: 1)
    scene.density = budget.particleDensity
    scene.thaws = true
    scene.updateWindows(surfaces, interval: 0.1)
    let fps = budget.frameRate(for: .calm, displayRate: 120)
    let dt = 1 / Float(fps)
    for _ in 0..<(fps * 20) { _ = scene.step(dt: dt, cursor: absent) }
    let capacity = 65536
    let sprites = UnsafeMutablePointer<Sprite>.allocate(capacity: capacity)
    let columns = UnsafeMutablePointer<Float>.allocate(capacity: capacity)
    let began = ProcessInfo.processInfo.systemUptime
    for _ in 0..<(fps * 10) {
        _ = scene.step(dt: dt, cursor: absent)
        check(scene.spriteCount <= capacity && scene.columnCount <= capacity, "Benchmark buffer capacity")
        _ = scene.writeSprites(into: sprites, capacity: capacity)
        _ = scene.writeLayers(into: columns, capacity: capacity)
    }
    let millisecondsPerSecond = (ProcessInfo.processInfo.systemUptime - began) * 100
    validate(scene)
    sprites.deallocate(); columns.deallocate()
    print(String(format: "Background %@: %d fps, %.0f%% particles, %.2f ms CPU work per simulated second", String(describing: budget), fps, budget.particleDensity * 100, millisecondsPerSecond))
}
