import XCTest
@testable import Snowhush

final class WeatherIntensityTests: XCTestCase {
    private func reading(_ code: Int = 63, precipitation: Double = 0, rain: Double = 0, snow: Double = 0,
                         interval: Double = 900) -> WeatherReading {
        WeatherReading(time: Date().timeIntervalSince1970, temperature: 1, code: code, isDay: 1,
                       precipitation: precipitation, rain: rain, snowfall: snow, cloudCover: 80,
                       windSpeed: 10, windDirection: 270, interval: interval)
    }

    func testMeasuredRateChangesIntensityWithinTheSameWeatherCode() {
        let drizzle = reading(precipitation: 0.025)
        let steady = reading(precipitation: 0.25)
        let heavy = reading(precipitation: 1.25)
        XCTAssertLessThan(drizzle.intensity, steady.intensity)
        XCTAssertLessThan(steady.intensity, heavy.intensity)
        let levels = [0.001, 0.01, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 100.0].map { reading(precipitation: $0).intensity }
        XCTAssertTrue(zip(levels, levels.dropFirst()).allSatisfy { $0 <= $1 })
        XCTAssertTrue(levels.allSatisfy { $0.isFinite && (0...1).contains($0) })
    }

    func testQuarterHourlyAndHourlyAmountsProduceTheSameIntensity() {
        let quarter = reading(precipitation: 0.25, interval: 900)
        let hourly = reading(precipitation: 1, interval: 3600)
        XCTAssertEqual(quarter.intensity, hourly.intensity, accuracy: 0.00001)
    }

    func testSnowAndRainUseConsistentWaterEquivalentUnits() {
        let rain = reading(precipitation: 1, rain: 1, interval: 3600)
        let snow = reading(73, snow: 0.7, interval: 3600)
        XCTAssertEqual(rain.intensity, snow.intensity, accuracy: 0.00001)
        XCTAssertGreaterThan(reading(73, snow: 1).intensity, reading(73, snow: 0.1).intensity)
    }

    func testRoundedZeroAmountsFallBackToTheConditionAndClearWeatherStaysClear() {
        XCTAssertGreaterThan(reading(61).intensity, 0)
        XCTAssertGreaterThan(reading(65).intensity, reading(61).intensity)
        XCTAssertEqual(reading(0, precipitation: 10, rain: 10).intensity, 0)
        XCTAssertEqual(reading(45, precipitation: 1).intensity, 0)
        XCTAssertEqual(reading(precipitation: 1000).intensity, 1)
    }

    func testAutomaticIntensityDoesNotInheritAnOldManualLevel() {
        let scene = WeatherScene.resolve(reading: reading(precipitation: 0.25), effects: .allWeather)
        for manual: Float in [0, 0.05, 0.5, 1] {
            let policy = WeatherIntensity(automatic: true, manual: manual)
            XCTAssertEqual(policy.precipitation(for: scene), scene.intensity)
            XCTAssertEqual(policy.atmosphere, 1)
        }
    }

    func testManualControlUsesTheSelectedLevelWithoutEnablingFilteredEffects() {
        let rain = reading(65, precipitation: 5)
        let scene = WeatherScene.resolve(reading: rain, effects: .allWeather)
        let manual = WeatherIntensity(automatic: false, manual: 0.25)
        XCTAssertEqual(manual.precipitation(for: scene), 0.25)
        XCTAssertEqual(manual.atmosphere, 0.5)
        XCTAssertEqual(manual.precipitation(for: .resolve(reading: rain, effects: .snowOnly)), 0)
        XCTAssertEqual(WeatherIntensity(automatic: false, manual: 0).precipitation(for: scene), 0)
        XCTAssertEqual(WeatherIntensity(automatic: false, manual: 0).atmosphere, 0)
    }

    func testForcedEffectsStillWorkWithoutAWeatherReading() {
        var effects = WeatherEffects.none
        effects[.snow] = .always
        let scene = WeatherScene.resolve(reading: nil, effects: effects)
        XCTAssertFalse(effects.needsWeather)
        XCTAssertEqual(WeatherIntensity(automatic: true, manual: 0).precipitation(for: scene), 0.5)
        XCTAssertEqual(WeatherIntensity(automatic: false, manual: 0.8).precipitation(for: scene), 0.8)
    }

    func testInvalidIntervalsCannotDriveTheScene() {
        for interval in [0, -1, Double.nan, Double.infinity] {
            let value = reading(precipitation: 0.1, interval: interval)
            XCTAssertFalse(value.isValid)
            XCTAssertTrue(WeatherScene.resolve(reading: value, effects: .allWeather).modes.isEmpty)
        }
    }
}
