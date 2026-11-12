import Foundation

struct WeatherPlace: Codable, Equatable, Sendable {
    let name: String
    let latitude: Double
    let longitude: Double
    var admin1: String?
    var country: String?

    var title: String {
        [name, admin1, country].compactMap { $0 }.reduce(into: [String]()) { parts, part in
            if !part.isEmpty && !parts.contains(part) { parts.append(part) }
        }.joined(separator: ", ")
    }

    var isValid: Bool { latitude.isFinite && longitude.isFinite && (-90...90).contains(latitude) && (-180...180).contains(longitude) }
}

enum WeatherCondition: String, CaseIterable, Sendable {
    case clear, partlyCloudy, cloudy, fog, drizzle, rain, snow, storm

    init?(code: Int) {
        switch code {
        case 0, 1: self = .clear
        case 2: self = .partlyCloudy
        case 3: self = .cloudy
        case 45, 48: self = .fog
        case 51, 53, 55, 56, 57: self = .drizzle
        case 61, 63, 65, 66, 67, 80, 81, 82: self = .rain
        case 71, 73, 75, 77, 85, 86: self = .snow
        case 95, 96, 97, 99: self = .storm
        default: return nil
        }
    }

    var key: String { "weather." + rawValue }

    func symbol(isDay: Bool) -> String {
        switch self {
        case .clear: return isDay ? "sun.max" : "moon.stars"
        case .partlyCloudy: return isDay ? "cloud.sun" : "cloud.moon"
        case .cloudy: return "cloud"
        case .fog: return "cloud.fog"
        case .drizzle: return "cloud.drizzle"
        case .rain: return "cloud.rain"
        case .snow: return "cloud.snow"
        case .storm: return "cloud.bolt.rain"
        }
    }
}

/// Current values use Open-Meteo's WMO codes. Time is requested as Unix time, independent of the Mac's timezone.
struct WeatherReading: Decodable, Equatable, Sendable {
    let time: Double
    let temperature: Double
    let code: Int
    let isDay: Int
    let precipitation: Double
    let rain: Double
    let snowfall: Double
    let cloudCover: Double
    let windSpeed: Double
    let windDirection: Double
    var interval: Double = 900

    enum CodingKeys: String, CodingKey {
        case time, precipitation, rain, snowfall, interval
        case temperature = "temperature_2m", code = "weather_code", isDay = "is_day"
        case cloudCover = "cloud_cover", windSpeed = "wind_speed_10m", windDirection = "wind_direction_10m"
    }

    var condition: WeatherCondition? { WeatherCondition(code: code) }
    var date: Date { Date(timeIntervalSince1970: time) }
    var isValid: Bool {
        condition != nil && [time, temperature, precipitation, rain, snowfall, cloudCover, windSpeed, windDirection, interval].allSatisfy(\.isFinite)
            && interval > 0 && (0...1).contains(isDay) && (0...100).contains(cloudCover) && precipitation >= 0 && rain >= 0 && snowfall >= 0 && windSpeed >= 0
    }

    var modes: [Mode] {
        switch condition {
        case .snow: return rain > 0 && snowfall > 0 ? [.snow, .rain] : [.snow]
        case .rain, .drizzle, .storm: return rain > 0 && snowfall > 0 ? [.snow, .rain] : [.rain]
        default: return [.snow] // No particles; intensity is zero. The atmosphere still draws.
        }
    }

    var intensity: Float {
        guard codedIntensity > 0 else { return 0 }
        // Current precipitation is an interval sum, not an hourly rate. Snow is in cm; Open-Meteo uses
        // 7 cm of snowfall per 10 mm of water equivalent. The total also includes convective showers.
        let water = max(precipitation, rain + snowfall * (10.0 / 7.0))
        guard water > 0, interval.isFinite, interval > 0 else { return codedIntensity }
        let rate = water * (3600 / interval)
        let stops: [(rate: Double, intensity: Float)] = [(0, 0.12), (0.1, 0.2), (0.5, 0.35), (2, 0.55), (5, 0.8), (10, 1)]
        for (lower, upper) in zip(stops, stops.dropFirst()) where rate <= upper.rate {
            let along = Float((rate - lower.rate) / (upper.rate - lower.rate))
            return lower.intensity + (upper.intensity - lower.intensity) * along
        }
        return 1
    }

    /// A weather code still provides a sensible level when tiny measured amounts have rounded to zero.
    private var codedIntensity: Float {
        switch code {
        case 51, 56, 77: return 0.2
        case 53, 61, 66, 71, 80, 85: return 0.35
        case 55, 57, 63, 73, 81, 95: return 0.6
        case 65, 67, 75, 82, 86, 96, 97, 99: return 0.9
        default: return 0
        }
    }

    /// Meteorological direction is where the wind comes from; its horizontal component points the other way.
    var wind: Float { Float(-sin(windDirection * .pi / 180) * min(windSpeed, 100) * 1.5) }
}

extension WeatherReading {
    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(time: try values.decode(Double.self, forKey: .time),
                  temperature: try values.decode(Double.self, forKey: .temperature),
                  code: try values.decode(Int.self, forKey: .code),
                  isDay: try values.decode(Int.self, forKey: .isDay),
                  precipitation: try values.decode(Double.self, forKey: .precipitation),
                  rain: try values.decode(Double.self, forKey: .rain),
                  snowfall: try values.decode(Double.self, forKey: .snowfall),
                  cloudCover: try values.decode(Double.self, forKey: .cloudCover),
                  windSpeed: try values.decode(Double.self, forKey: .windSpeed),
                  windDirection: try values.decode(Double.self, forKey: .windDirection),
                  interval: try values.decodeIfPresent(Double.self, forKey: .interval) ?? 900)
    }
}

struct WeatherSnapshot: Equatable, Sendable {
    let place: WeatherPlace
    let reading: WeatherReading
    /// One hour is deliberately short: do not keep showing yesterday's storm after a network failure or sleep.
    func isFresh(at now: Date = Date()) -> Bool { (-300...3600).contains(now.timeIntervalSince(reading.date)) }
}

enum WeatherError: Error {
    case unavailable, invalidResponse
}

protocol WeatherProviding: Sendable {
    func current(at place: WeatherPlace) async throws -> WeatherSnapshot
}

struct WeatherAPI: WeatherProviding {
    var session: URLSession = .shared

    func search(_ name: String) async throws -> [WeatherPlace] {
        struct Response: Decodable { var results: [WeatherPlace]? }
        let language = Locale.current.language.languageCode?.identifier ?? "en"
        let data = try await data(from: "https://geocoding-api.open-meteo.com/v1/search", query: [
            "name": name, "count": "8", "language": language, "format": "json",
        ])
        return try JSONDecoder().decode(Response.self, from: data).results?.filter(\.isValid) ?? []
    }

    func current(at place: WeatherPlace) async throws -> WeatherSnapshot {
        guard place.isValid else { throw WeatherError.invalidResponse }
        struct Response: Decodable { let current: WeatherReading }
        let data = try await data(from: "https://api.open-meteo.com/v1/forecast", query: [
            "latitude": String(place.latitude), "longitude": String(place.longitude),
            "current": "temperature_2m,weather_code,is_day,precipitation,rain,snowfall,cloud_cover,wind_speed_10m,wind_direction_10m",
            "timeformat": "unixtime", "wind_speed_unit": "kmh", "precipitation_unit": "mm", "forecast_days": "1",
        ])
        let reading = try JSONDecoder().decode(Response.self, from: data).current
        let snapshot = WeatherSnapshot(place: place, reading: reading)
        guard reading.isValid, snapshot.isFresh() else { throw WeatherError.invalidResponse }
        return snapshot
    }

    private func data(from endpoint: String, query: [String: String]) async throws -> Data {
        var components = URLComponents(string: endpoint)!
        components.queryItems = query.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) }
        var request = URLRequest(url: components.url!)
        request.timeoutInterval = 20
        request.cachePolicy = .reloadIgnoringLocalCacheData
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200 else { throw WeatherError.unavailable }
        return data
    }
}
