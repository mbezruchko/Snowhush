import AppKit
import CoreLocation

/// One location fix per refresh, at city accuracy, only while enabled and awake.
@MainActor
final class WeatherSync: NSObject, @preconcurrency CLLocationManagerDelegate {
    enum Status: String {
        case off, manual, locating, updating, ready, failed, locationDenied, locationUnavailable, stale
        var key: String { "weather.status." + rawValue }
    }

    private(set) var snapshot: WeatherSnapshot?
    private(set) var status = Status.off
    var changed: () -> Void = {}
    var enabled: Bool { Preferences.weatherEnabled }
    var needsWeather: Bool { Preferences.weatherEffects.needsWeather }
    private let provider: any WeatherProviding
    private lazy var location: CLLocationManager = {
        let manager = CLLocationManager()
        manager.desiredAccuracy = kCLLocationAccuracyKilometer
        manager.delegate = self
        return manager
    }()
    private var polling: Timer?
    private var locationTimeout: Timer?
    private var task: Task<Void, Never>?
    private var generation = UUID()
    private var requestingLocation = false
    private var suspended = false

    init(provider: any WeatherProviding = WeatherAPI()) {
        self.provider = provider
        super.init()
    }

    func start() {
        guard enabled, polling == nil else { return }
        guard needsWeather else {
            status = .manual
            changed()
            return
        }
        refresh()
        let timer = Timer(timeInterval: 15 * 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
        RunLoop.main.add(timer, forMode: .common)
        polling = timer
    }

    func setEnabled(_ enabled: Bool) {
        Preferences.weatherEnabled = enabled
        restart()
    }

    func setEffects(_ effects: WeatherEffects) {
        let wasEnabled = enabled
        let neededWeather = needsWeather
        Preferences.weatherEffects = effects
        Preferences.weatherEnabled = true
        if !wasEnabled || neededWeather != effects.needsWeather { restart() }
        else { changed() }
    }

    func setAutomaticIntensity(_ automatic: Bool) {
        Preferences.weatherAutomaticIntensity = automatic
        changed()
    }

    func setManualIntensity(_ intensity: Double) {
        Preferences.intensity = min(max(intensity, 0), 1)
        Preferences.weatherAutomaticIntensity = false
        changed()
    }

    func useAutomaticLocation() {
        Preferences.weatherAutomatic = true
        Preferences.weatherEnabled = true
        NSApp.activate()
        restart()
    }

    func use(_ place: WeatherPlace) {
        guard place.isValid else { return }
        Preferences.weatherPlace = place
        Preferences.weatherAutomatic = false
        Preferences.weatherEnabled = true
        restart()
    }

    private func restart() {
        cancel()
        polling?.invalidate()
        polling = nil
        snapshot = nil
        status = .off
        changed()
        start()
    }

    func setSuspended(_ suspended: Bool) {
        guard suspended != self.suspended else { return }
        self.suspended = suspended
        if suspended { cancel() }
        else if enabled { refresh() }
    }

    /// Keep a recent scene on failure, but never let an old reading keep driving the desktop.
    func refresh() {
        guard enabled, !suspended, task == nil, !requestingLocation else { return }
        guard needsWeather else {
            status = .manual
            changed()
            return
        }
        if let snapshot, !snapshot.isFresh() {
            self.snapshot = nil
            status = .stale
            changed()
        }
        if !Preferences.weatherAutomatic {
            guard let place = Preferences.weatherPlace else {
                status = .locationUnavailable
                changed()
                return
            }
            fetch(place)
        } else {
            requestingLocation = true
            status = .locating
            changed()
            let timer = Timer(timeInterval: 25, repeats: false) { [weak self] _ in
                Task { @MainActor in
                    guard let self, self.requestingLocation else { return }
                    self.stopLocation()
                    self.status = .locationUnavailable
                    self.changed()
                }
            }
            RunLoop.main.add(timer, forMode: .common)
            locationTimeout = timer
            authorizeLocation()
        }
    }

    private func authorizeLocation() {
        guard requestingLocation else { return }
        switch location.authorizationStatus {
        case .notDetermined: location.requestWhenInUseAuthorization()
        case .authorizedAlways, .authorizedWhenInUse: location.requestLocation()
        case .restricted, .denied:
            stopLocation()
            snapshot = nil
            status = .locationDenied
            changed()
        @unknown default:
            stopLocation()
            status = .locationUnavailable
            changed()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        if manager.authorizationStatus == .denied || manager.authorizationStatus == .restricted {
            guard enabled, needsWeather, Preferences.weatherAutomatic else { return }
            cancel()
            snapshot = nil
            status = .locationDenied
            changed()
        } else if requestingLocation {
            authorizeLocation()
        } else if enabled, needsWeather, Preferences.weatherAutomatic, !suspended, status == .locationUnavailable {
            refresh()
        }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard enabled, needsWeather, !suspended, requestingLocation,
              let point = locations.last, point.horizontalAccuracy >= 0, abs(point.timestamp.timeIntervalSinceNow) < 300
        else { return }
        stopLocation()
        // Only rounded coordinates leave the device; automatic locations are not persisted.
        fetch(WeatherPlace(name: tr("weather.here"),
                           latitude: (point.coordinate.latitude * 100).rounded() / 100,
                           longitude: (point.coordinate.longitude * 100).rounded() / 100))
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        guard requestingLocation else { return }
        stopLocation()
        status = (error as? CLError)?.code == .denied ? .locationDenied : .locationUnavailable
        changed()
    }

    private func fetch(_ place: WeatherPlace) {
        let ticket = generation
        status = .updating
        changed()
        task = Task { [weak self, provider] in
            do {
                let next = try await provider.current(at: place)
                guard let self, !Task.isCancelled, ticket == generation, enabled, needsWeather, !suspended else { return }
                guard next.reading.isValid, next.isFresh() else { throw WeatherError.invalidResponse }
                snapshot = next
                status = .ready
                task = nil
                changed()
            } catch {
                guard let self, !Task.isCancelled, ticket == generation else { return }
                if snapshot?.isFresh() == false { snapshot = nil }
                status = .failed
                task = nil
                changed()
            }
        }
    }

    private func stopLocation() {
        requestingLocation = false
        locationTimeout?.invalidate()
        locationTimeout = nil
        location.stopUpdatingLocation()
    }

    private func cancel() {
        generation = UUID()
        task?.cancel()
        task = nil
        if requestingLocation { stopLocation() }
    }
}
