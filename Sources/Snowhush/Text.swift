import Foundation

/// The text for `key` from `locales/`, which build.sh ships as the app's `.lproj` tables.
func tr(_ key: String) -> String {
    Bundle.main.localizedString(forKey: key, value: nil, table: nil)
}

/// The text for `prefix` and what falls, in `Mode.allCases` order: "windows.snow.autumn".
func tr(_ prefix: String, _ modes: [Mode]) -> String {
    tr(([prefix] + Mode.allCases.filter(modes.contains).map(\.key)).joined(separator: "."))
}

extension Mode {
    var key: String { ["snow", "autumn", "rain", "blossom"][rawValue] }
}
