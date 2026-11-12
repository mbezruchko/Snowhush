import AppKit
import Metal
import XCTest
@testable import Snowhush

final class WeatherTests: XCTestCase {
    private func reading(_ code: Int, time: Double = Date().timeIntervalSince1970, rain: Double = 0, snow: Double = 0,
                         day: Int = 1, clouds: Double = 80, direction: Double = 270) -> WeatherReading {
        WeatherReading(time: time, temperature: 4, code: code, isDay: day, precipitation: rain + snow,
                       rain: rain, snowfall: snow, cloudCover: clouds, windSpeed: 20, windDirection: direction)
    }

    func testWMOCodesAndPrecipitation() {
        let groups: [(WeatherCondition, [Int])] = [
            (.clear, [0, 1]), (.partlyCloudy, [2]), (.cloudy, [3]), (.fog, [45, 48]),
            (.drizzle, [51, 53, 55, 56, 57]), (.rain, [61, 63, 65, 66, 67, 80, 81, 82]),
            (.snow, [71, 73, 75, 77, 85, 86]), (.storm, [95, 96, 97, 99]),
        ]
        for (condition, codes) in groups {
            for code in codes {
                let value = reading(code)
                XCTAssertEqual(value.condition, condition)
                XCTAssertTrue(value.isValid)
                let precipitates = [.drizzle, .rain, .snow, .storm].contains(condition)
                XCTAssertEqual(value.intensity > 0, precipitates, "WMO \(code)")
            }
        }
        XCTAssertNil(WeatherCondition(code: 123))
        XCTAssertFalse(reading(123).isValid)
        XCTAssertEqual(reading(71).modes, [.snow])
        XCTAssertEqual(reading(61).modes, [.rain])
        XCTAssertEqual(reading(73, rain: 0.2, snow: 0.1).modes, [.snow, .rain])
        XCTAssertLessThan(reading(61).intensity, reading(65).intensity)
        XCTAssertEqual(reading(0, rain: 1).intensity, 0, "A preceding interval's rain must not make a clear sky rainy.")
    }

    func testWindDirectionAndAge() {
        XCTAssertGreaterThan(reading(71, direction: 270).wind, 0)
        XCTAssertLessThan(reading(71, direction: 90).wind, 0)
        let now = Date()
        let place = WeatherPlace(name: "Test", latitude: 55, longitude: 37)
        XCTAssertTrue(WeatherSnapshot(place: place, reading: reading(0, time: now.timeIntervalSince1970 - 900)).isFresh(at: now))
        XCTAssertFalse(WeatherSnapshot(place: place, reading: reading(0, time: now.timeIntervalSince1970 - 3601)).isFresh(at: now))
        XCTAssertFalse(WeatherSnapshot(place: place, reading: reading(0, time: now.timeIntervalSince1970 + 301)).isFresh(at: now))
        XCTAssertFalse(WeatherPlace(name: "Bad", latitude: .nan, longitude: 0).isValid)
        XCTAssertFalse(reading(0, clouds: 101).isValid)
    }

    func testCurrentWeatherDecoding() throws {
        let json = #"{"time":1794470400,"temperature_2m":-2.5,"weather_code":73,"is_day":0,"precipitation":0.5,"rain":0,"snowfall":0.3,"cloud_cover":100,"wind_speed_10m":12,"wind_direction_10m":90}"#
        let value = try JSONDecoder().decode(WeatherReading.self, from: Data(json.utf8))
        XCTAssertEqual(value.condition, .snow)
        XCTAssertEqual(value.temperature, -2.5)
        XCTAssertEqual(value.isDay, 0)
        XCTAssertEqual(value.interval, 900, "Older readings without an interval retain the fifteen-minute default")
        XCTAssertTrue(value.isValid)
        XCTAssertThrowsError(try JSONDecoder().decode(WeatherReading.self, from: Data("{\"weather_code\":null}".utf8)))
    }

    func testAtmosphereFadesAndHonorsReducedMotion() {
        var sky = WeatherAtmosphere(scene: .resolve(reading: reading(45), effects: .allWeather))
        sky.step(dt: 0.05, calm: false)
        XCTAssertGreaterThan(sky.layers.y, 0)
        XCTAssertLessThan(sky.layers.y, 1)
        sky.step(dt: 0.05, calm: true)
        XCTAssertEqual(sky.layers.y, 1)
        sky.scene = .empty
        for _ in 0..<600 { sky.step(dt: 0.05, calm: false) }
        XCTAssertFalse(sky.isVisible)
        sky.scene = .resolve(reading: reading(0, day: 0, clouds: 0), effects: .allWeather)
        sky.step(dt: 0.05, calm: true)
        XCTAssertEqual(sky.environment.x, 1)
        XCTAssertEqual(sky.layers.z, 0, "Night must not show sunlight.")
        sky.strength = 0
        sky.step(dt: 0.05, calm: true)
        XCTAssertFalse(sky.isVisible)
    }

    /// This compiles the actual combined Metal source and draws every atmosphere to catch errors Swift cannot see.
    func testMetalAtmospheres() throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw XCTSkip("Metal is unavailable on this runner") }
        let library = try device.makeLibrary(source: shaderSource + "\n" + weatherShaderSource, options: nil)
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "weatherVertex")
        descriptor.fragmentFunction = library.makeFunction(name: "weatherFragment")
        descriptor.colorAttachments[0].pixelFormat = .rgba8Unorm
        let pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
        let textureDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: 640, height: 400, mipmapped: false)
        textureDescriptor.usage = [.renderTarget]
        textureDescriptor.storageMode = .shared
        let target = try XCTUnwrap(device.makeTexture(descriptor: textureDescriptor))
        let queue = try XCTUnwrap(device.makeCommandQueue())
        for (code, day) in [(0, 1), (0, 0), (2, 1), (3, 1), (45, 1), (95, 1)] {
            var sky = WeatherAtmosphere(scene: .resolve(reading: reading(code, day: day, clouds: code == 0 ? 0 : 80), effects: .allWeather))
            sky.step(dt: 1, calm: true)
            var uniforms = WeatherUniforms(layers: sky.layers, environment: sky.environment,
                viewport: SIMD4<Float>(640, 400, 6.15, 1), lightning: sky.lightning.flash)
            let pass = MTLRenderPassDescriptor()
            pass.colorAttachments[0].texture = target
            pass.colorAttachments[0].loadAction = .clear
            pass.colorAttachments[0].storeAction = .store
            pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 0)
            let buffer = try XCTUnwrap(queue.makeCommandBuffer())
            let encoder = try XCTUnwrap(buffer.makeRenderCommandEncoder(descriptor: pass))
            encoder.setRenderPipelineState(pipeline)
            encoder.setFragmentBytes(&uniforms, length: MemoryLayout<WeatherUniforms>.stride, index: 0)
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            encoder.endEncoding()
            buffer.commit()
            buffer.waitUntilCompleted()
            XCTAssertNil(buffer.error)
            var pixels = [UInt8](repeating: 0, count: 640 * 400 * 4)
            target.getBytes(&pixels, bytesPerRow: 640 * 4, from: MTLRegionMake2D(0, 0, 640, 400), mipmapLevel: 0)
            let alphas = stride(from: 3, to: pixels.count, by: 4).map { pixels[$0] }
            XCTAssertGreaterThan(alphas.max() ?? 0, 0, "Weather \(code) must be visible")
            XCTAssertLessThan(alphas.max() ?? 255, 230, "The desktop must remain visible even under the lunar disc")
        }
    }

    @MainActor
    func testManualIntensityDisablesAutomationWithoutChangingEffectRules() async {
        let saved = savePreferences()
        let provider = PendingWeather()
        let sync = WeatherSync(provider: provider)
        defer { sync.setEnabled(false); restorePreferences(saved) }
        UserDefaults.standard.removeObject(forKey: "weatherAutomaticIntensity")
        XCTAssertTrue(Preferences.weatherAutomaticIntensity)
        sync.setEffects(.fogOnly)
        sync.setManualIntensity(0.7)
        XCTAssertFalse(Preferences.weatherAutomaticIntensity)
        XCTAssertEqual(Preferences.intensity, 0.7, accuracy: 0.0001)
        XCTAssertEqual(Preferences.weatherEffects, .fogOnly)
        sync.setAutomaticIntensity(true)
        XCTAssertTrue(Preferences.weatherAutomaticIntensity)
        XCTAssertEqual(Preferences.intensity, 0.7, accuracy: 0.0001, "The saved manual level is kept when automation resumes")
        XCTAssertEqual(Preferences.weatherEffects, .fogOnly)
        let requests = await provider.requests
        XCTAssertEqual(requests, 0, "Intensity changes alone do not start location or weather requests")
    }

    @MainActor
    func testManualFogDoesNotRequestWeather() async {
        let saved = savePreferences()
        let provider = PendingWeather()
        let sync = WeatherSync(provider: provider)
        defer { sync.setEnabled(false); restorePreferences(saved) }
        sync.setEffects(.fogOnly)
        sync.refresh()
        for _ in 0..<20 { await Task.yield() }
        XCTAssertTrue(sync.enabled)
        XCTAssertFalse(sync.needsWeather)
        XCTAssertEqual(sync.status, .manual)
        let requests = await provider.requests
        XCTAssertEqual(requests, 0)
        XCTAssertNil(sync.snapshot)
        XCTAssertEqual(Preferences.weatherEffects, .fogOnly)
    }

    @MainActor
    func testSwitchingToManualEffectsRejectsOldWeather() async {
        let saved = savePreferences()
        let provider = PendingWeather()
        let sync = WeatherSync(provider: provider)
        defer { sync.setEnabled(false); restorePreferences(saved) }
        let place = WeatherPlace(name: "A", latitude: 55, longitude: 37)
        sync.use(place)
        await provider.waitUntilRequested()
        sync.setEffects(.fogOnly)
        await provider.finish(WeatherSnapshot(place: place, reading: reading(95)))
        for _ in 0..<20 { await Task.yield() }
        XCTAssertNil(sync.snapshot)
        XCTAssertEqual(sync.status, .manual)
        XCTAssertEqual(Preferences.weatherEffects, .fogOnly)
    }

    @MainActor
    func testMissingManualCityDoesNotRequestLocation() {
        let saved = savePreferences()
        let sync = WeatherSync(provider: PendingWeather())
        defer { sync.setEnabled(false); restorePreferences(saved) }
        Preferences.weatherAutomatic = false
        Preferences.weatherPlace = nil
        sync.setEnabled(true)
        XCTAssertEqual(sync.status, .locationUnavailable)
        XCTAssertNil(sync.snapshot)
    }

    @MainActor
    func testTurningOffRejectsAnInFlightResponse() async {
        let saved = savePreferences()
        let provider = PendingWeather()
        let sync = WeatherSync(provider: provider)
        defer { sync.setEnabled(false); restorePreferences(saved) }
        let place = WeatherPlace(name: "A", latitude: 55, longitude: 37)
        sync.use(place)
        await provider.waitUntilRequested()
        sync.setEnabled(false)
        await provider.finish(WeatherSnapshot(place: place, reading: reading(95)))
        for _ in 0..<20 { await Task.yield() }
        XCTAssertNil(sync.snapshot)
        XCTAssertEqual(sync.status, .off)
    }

    @MainActor
    func testSuspendingCancelsAndResumingRefreshes() async {
        let saved = savePreferences()
        let provider = PendingWeather()
        let sync = WeatherSync(provider: provider)
        defer { sync.setEnabled(false); restorePreferences(saved) }
        let place = WeatherPlace(name: "A", latitude: 55, longitude: 37)
        sync.use(place)
        await provider.waitUntilRequested()
        sync.setSuspended(true)
        await provider.finish(WeatherSnapshot(place: place, reading: reading(95)))
        for _ in 0..<20 { await Task.yield() }
        XCTAssertNil(sync.snapshot)
        sync.setSuspended(false)
        await provider.waitUntilRequested()
        let ready = expectation(description: "Fresh weather after wake")
        sync.changed = { if sync.status == .ready { ready.fulfill() } }
        await provider.finish(WeatherSnapshot(place: place, reading: reading(0)))
        await fulfillment(of: [ready], timeout: 2)
        XCTAssertEqual(sync.snapshot?.reading.condition, .clear)
    }

    private let preferenceKeys = ["weatherEnabled", "weatherAutomatic", "weatherPlace", "weatherEffects", "weatherAutomaticIntensity", "intensity"]
    private func savePreferences() -> [String: Any] {
        let saved = preferenceKeys.reduce(into: [String: Any]()) { if let value = UserDefaults.standard.object(forKey: $1) { $0[$1] = value } }
        Preferences.weatherEffects = .allWeather
        return saved
    }
    private func restorePreferences(_ saved: [String: Any]) {
        for key in preferenceKeys {
            if let value = saved[key] { UserDefaults.standard.set(value, forKey: key) }
            else { UserDefaults.standard.removeObject(forKey: key) }
        }
    }
}

/// Deliberately ignores cancellation, like a response already queued when a mode is changed.
private actor PendingWeather: WeatherProviding {
    private(set) var requests = 0
    private var pending: CheckedContinuation<WeatherSnapshot, Error>?
    private var waiter: CheckedContinuation<Void, Never>?
    func current(at place: WeatherPlace) async throws -> WeatherSnapshot {
        requests += 1
        return try await withCheckedThrowingContinuation { continuation in
            pending = continuation
            waiter?.resume()
            waiter = nil
        }
    }
    func waitUntilRequested() async {
        if pending != nil { return }
        await withCheckedContinuation { waiter = $0 }
    }
    func finish(_ snapshot: WeatherSnapshot) {
        pending?.resume(returning: snapshot)
        pending = nil
    }
}
