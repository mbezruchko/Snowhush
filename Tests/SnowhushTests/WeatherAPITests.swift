import Foundation
import XCTest
@testable import Snowhush

final class WeatherAPITests: XCTestCase {
    private func api(body: String, status: Int = 200) -> WeatherAPI {
        WeatherFixtureProtocol.body = Data(body.utf8)
        WeatherFixtureProtocol.status = status
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [WeatherFixtureProtocol.self]
        return WeatherAPI(session: URLSession(configuration: configuration))
    }

    private func body(code: Int = 63, time: Double = Date().timeIntervalSince1970) -> String {
        """
        {"current":{"time":\(time),"temperature_2m":4.5,"weather_code":\(code),"is_day":1,
        "precipitation":0.3,"rain":0.3,"snowfall":0,"cloud_cover":95,"wind_speed_10m":8,"wind_direction_10m":240}}
        """
    }

    func testForecastRequestAndResponse() async throws {
        let service = api(body: body())
        defer { service.session.invalidateAndCancel() }
        let place = WeatherPlace(name: "Test", latitude: 55.75, longitude: 37.62)
        let snapshot = try await service.current(at: place)
        XCTAssertEqual(snapshot.reading.condition, .rain)
        XCTAssertEqual(snapshot.place, place)
        let url = try XCTUnwrap(WeatherFixtureProtocol.requestURL)
        let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        XCTAssertEqual(query.first { $0.name == "timeformat" }?.value, "unixtime")
        XCTAssertEqual(query.first { $0.name == "wind_speed_unit" }?.value, "kmh")
        XCTAssertEqual(query.first { $0.name == "latitude" }?.value, "55.75")
        XCTAssertTrue(query.first { $0.name == "current" }?.value?.contains("is_day") == true)
    }

    func testRejectsHTTPFailureAndStaleOrUnknownWeather() async {
        for (json, status) in [(body(), 503), (body(time: Date().timeIntervalSince1970 - 7200), 200), (body(code: 999), 200), ("{}", 200)] {
            let service = api(body: json, status: status)
            do {
                _ = try await service.current(at: WeatherPlace(name: "Test", latitude: 55, longitude: 37))
                XCTFail("Invalid weather must not drive the scene")
            } catch { /* Expected: callers can keep the last recent reading or show an error. */ }
            service.session.invalidateAndCancel()
        }
    }

    func testCitySearchKeepsDistinctMatchesAndFiltersBadCoordinates() async throws {
        let service = api(body: #"{"results":[{"name":"Springfield","latitude":39.8,"longitude":-89.6,"admin1":"Illinois","country":"United States"},{"name":"Springfield","latitude":42.1,"longitude":-72.6,"admin1":"Massachusetts","country":"United States"},{"name":"Bad","latitude":999,"longitude":0}]}"#)
        defer { service.session.invalidateAndCancel() }
        let places = try await service.search("Springfield")
        XCTAssertEqual(places.count, 2)
        XCTAssertNotEqual(places[0].title, places[1].title)
        XCTAssertEqual(places[0].title, "Springfield, Illinois, United States")
    }

    func testEmptyCitySearchResults() async throws {
        let service = api(body: "{\"generationtime_ms\":0.1}")
        defer { service.session.invalidateAndCancel() }
        let matches = try await service.search("Unknown")
        XCTAssertTrue(matches.isEmpty)
    }
}

private final class WeatherFixtureProtocol: URLProtocol {
    static var body = Data()
    static var status = 200
    static var requestURL: URL?
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.requestURL = request.url
        let response = HTTPURLResponse(url: request.url!, statusCode: Self.status, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.body)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}
