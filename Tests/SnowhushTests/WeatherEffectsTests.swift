import XCTest
@testable import Snowhush

final class WeatherEffectsTests: XCTestCase {
    private func reading(_ code: Int, day: Int = 1, clouds: Double = 100, rain: Double = 0, snow: Double = 0) -> WeatherReading {
        WeatherReading(time: Date().timeIntervalSince1970, temperature: 0, code: code, isDay: day,
                       precipitation: rain + snow, rain: rain, snowfall: snow, cloudCover: clouds,
                       windSpeed: 20, windDirection: 270)
    }

    func testOnlySnowWhenSnowingAndNothingWhenRaining() {
        for code in [71, 73, 75, 77, 85, 86] {
            let scene = WeatherScene.resolve(reading: reading(code), effects: .snowOnly)
            XCTAssertEqual(scene.modes, [.snow])
            XCTAssertGreaterThan(scene.intensity, 0)
            XCTAssertEqual(scene.layers, .zero)
            XCTAssertEqual(scene.night, 0)
        }
        for code in [0, 1, 2, 3, 45, 48, 51, 55, 61, 63, 65, 80, 82, 95, 99] {
            let scene = WeatherScene.resolve(reading: reading(code, day: 0), effects: .snowOnly)
            XCTAssertTrue(scene.modes.isEmpty, "No particles for WMO \(code)")
            XCTAssertEqual(scene.intensity, 0)
            XCTAssertEqual(scene.layers, .zero)
            XCTAssertEqual(scene.night, 0)
            var sky = WeatherAtmosphere(scene: scene)
            sky.step(dt: 1, calm: true)
            XCTAssertFalse(sky.isVisible, "Disabling atmosphere must also hide moon glow at night")
        }
    }

    func testForcedFogWithoutLocationOrWeather() {
        XCTAssertFalse(WeatherEffects.fogOnly.needsWeather)
        let scene = WeatherScene.resolve(reading: nil, effects: .fogOnly)
        XCTAssertEqual(scene.layers, SIMD4(0, 1, 0, 0))
        XCTAssertTrue(scene.modes.isEmpty)
        XCTAssertEqual(scene.intensity, 0)
        var sky = WeatherAtmosphere(scene: scene)
        sky.step(dt: 1, calm: true)
        XCTAssertTrue(sky.isVisible)
        XCTAssertEqual(sky.layers.y, 1)
    }

    func testAllEffectsCanBeForcedIndependently() {
        for effect in WeatherEffect.allCases {
            var effects = WeatherEffects.none
            effects[effect] = .always
            XCTAssertFalse(effects.needsWeather)
            let scene = WeatherScene.resolve(reading: nil, effects: effects)
            switch effect {
            case .snow: XCTAssertEqual(scene.modes, [.snow])
            case .rain: XCTAssertEqual(scene.modes, [.rain])
            case .clouds: XCTAssertEqual(scene.layers, SIMD4(1, 0, 0, 0))
            case .fog: XCTAssertEqual(scene.layers, SIMD4(0, 1, 0, 0))
            case .sunlight: XCTAssertEqual(scene.layers, SIMD4(0, 0, 1, 0))
            case .lightning: XCTAssertEqual(scene.layers, SIMD4(0, 0, 0, 1))
            case .night: XCTAssertEqual(scene.night, 1)
            }
        }
    }

    func testForcedFogAndWeatherSnowCanBeCombined() {
        var effects = WeatherEffects.snowOnly
        effects[.fog] = .always
        XCTAssertTrue(effects.needsWeather)
        let rain = WeatherScene.resolve(reading: reading(65), effects: effects)
        XCTAssertTrue(rain.modes.isEmpty)
        XCTAssertEqual(rain.layers, SIMD4(0, 1, 0, 0))
        let snow = WeatherScene.resolve(reading: reading(75), effects: effects)
        XCTAssertEqual(snow.modes, [.snow])
        XCTAssertEqual(snow.layers.y, 1)
        let offline = WeatherScene.resolve(reading: nil, effects: effects)
        XCTAssertTrue(offline.modes.isEmpty)
        XCTAssertEqual(offline.layers.y, 1)
    }

    func testFilteringMixedPrecipitation() {
        let mixed = reading(73, rain: 0.1, snow: 0.1)
        XCTAssertEqual(WeatherScene.resolve(reading: mixed, effects: .allWeather).modes, [.snow, .rain])
        XCTAssertEqual(WeatherScene.resolve(reading: mixed, effects: .snowOnly).modes, [.snow])
        var rainOnly = WeatherEffects.none
        rainOnly[.rain] = .weather
        XCTAssertEqual(WeatherScene.resolve(reading: mixed, effects: rainOnly).modes, [.rain])
    }

    func testDisabledFogAndAllOffOverrideActualWeather() {
        var effects = WeatherEffects.allWeather
        effects[.fog] = .off
        XCTAssertEqual(WeatherScene.resolve(reading: reading(45), effects: effects).layers.y, 0)
        for code in [0, 45, 65, 75, 95] {
            let scene = WeatherScene.resolve(reading: reading(code, day: 0), effects: .none)
            XCTAssertTrue(scene.modes.isEmpty)
            XCTAssertEqual(scene.layers, .zero)
            XCTAssertEqual(scene.night, 0)
            XCTAssertEqual(scene.intensity, 0)
        }
    }

    func testTurningFogOffFadesItCompletely() {
        var sky = WeatherAtmosphere(scene: .resolve(reading: nil, effects: .fogOnly))
        sky.step(dt: 1, calm: true)
        sky.scene = .empty
        sky.step(dt: 0.05, calm: false)
        XCTAssertGreaterThan(sky.layers.y, 0)
        XCTAssertLessThan(sky.layers.y, 1)
        for _ in 0..<600 { sky.step(dt: 0.05, calm: false) }
        XCTAssertFalse(sky.isVisible)
    }

    func testCustomRulesSurviveSaving() throws {
        var effects = WeatherEffects.snowOnly
        effects[.fog] = .always
        effects[.night] = .weather
        let encoded = try JSONEncoder().encode(effects)
        XCTAssertEqual(try JSONDecoder().decode(WeatherEffects.self, from: encoded), effects)
    }

    func testMissingSavedRulesDoNotEnableUnchosenEffects() throws {
        let saved = Data(#"{"rules":{"snow":"weather"}}"#.utf8)
        let effects = try JSONDecoder().decode(WeatherEffects.self, from: saved)
        XCTAssertEqual(effects, .snowOnly, "Sparse settings must not silently enable more effects")
    }

    func testForcingMoonDoesNotChangeDaytimeFogOrClouds() {
        var effects = WeatherEffects.allWeather
        effects[.night] = .always
        let day = reading(45, day: 1)
        let scene = WeatherScene.resolve(reading: day, effects: effects)
        XCTAssertEqual(scene.night, 1)
        XCTAssertEqual(scene.nightTint, 0)
        effects[.night] = .off
        let withoutMoon = WeatherScene.resolve(reading: day, effects: effects)
        XCTAssertEqual(scene.layers, withoutMoon.layers)
        XCTAssertEqual(scene.nightTint, withoutMoon.nightTint)
    }
}
