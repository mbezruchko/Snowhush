import XCTest
@testable import Snowhush

final class WeatherLightningTests: XCTestCase {
    func testFirstStrikeIsQuickAndThenTheSceneRests() {
        var lightning = WeatherLightning()
        var active = [Float]()
        for frame in 0..<600 {
            lightning.step(dt: 1 / 60, enabled: true, calm: false)
            if lightning.flash.x > 0.01 { active.append(Float(frame + 1) / 60) }
        }
        XCTAssertGreaterThan(active.count, 0)
        XCTAssertLessThan(active.first ?? 10, 2, "The preview should not look broken while waiting nineteen seconds")
        XCTAssertLessThan((active.last ?? 10) - (active.first ?? 0), 0.7)
        XCTAssertEqual(lightning.flash.x, 0)
    }

    func testStrikesVaryInPositionShapeAndSpacing() {
        var lightning = WeatherLightning()
        var previous: Float = 0
        var starts: [Int] = []
        var positions: [Float] = []
        var seeds: [Float] = []
        for frame in 0..<6000 {
            lightning.step(dt: 1 / 60, enabled: true, calm: false)
            XCTAssertTrue((0...1).contains(lightning.flash.x))
            if lightning.flash.x > 0.01, previous <= 0.01,
               starts.last.map({ frame - $0 > 60 }) ?? true {
                starts.append(frame)
                positions.append(lightning.flash.y)
                seeds.append(lightning.flash.z)
            }
            previous = lightning.flash.x
        }
        XCTAssertGreaterThan(starts.count, 4)
        let intervals = zip(starts, starts.dropFirst()).map { $1 - $0 }
        XCTAssertTrue(intervals.allSatisfy { (12 * 60 - 2...21 * 60 + 2).contains($0) })
        XCTAssertGreaterThan(Set(intervals).count, 1)
        XCTAssertGreaterThan(Set(positions).count, 1)
        XCTAssertGreaterThan(Set(seeds).count, 1)
    }

    func testTurningOffAndReducedMotionCancelAnActiveStrike() {
        for calm in [false, true] {
            var lightning = WeatherLightning()
            lightning.step(dt: 1.25, enabled: true, calm: false)
            XCTAssertGreaterThan(lightning.flash.x, 0.5)
            lightning.step(dt: 0.01, enabled: calm, calm: calm)
            XCTAssertEqual(lightning.flash, .zero)
            lightning.step(dt: 0.05, enabled: true, calm: false)
            XCTAssertEqual(lightning.flash.x, 0, "Re-enabling should start a fresh sequence, not resume in the middle of a flash")
        }
    }

    func testAtmosphereHonorsOffIntensityAndReducedMotion() {
        var effects = WeatherEffects.none
        effects[.lightning] = .always
        let scene = WeatherScene.resolve(reading: nil, effects: effects)
        var sky = WeatherAtmosphere(scene: scene)
        sky.step(dt: 1.25, calm: false)
        XCTAssertGreaterThan(sky.lightning.flash.x, 0.5)
        sky.strength = 0
        sky.step(dt: 0.01, calm: false)
        XCTAssertEqual(sky.lightning.flash, .zero)
        sky.strength = 1
        sky.step(dt: 1.25, calm: true)
        XCTAssertEqual(sky.lightning.flash, .zero)
    }
}
