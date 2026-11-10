import AppKit

/// A macOS 26 menu's measures, taken from one at 2x, and the older ones before it: rows 24 pt high, highlights
/// rounded 7 pt.
enum MenuMetrics {
    static let modern = ProcessInfo.processInfo.isOperatingSystemAtLeast(OperatingSystemVersion(majorVersion: 26, minorVersion: 0, patchVersion: 0))
    static let rowHeight: CGFloat = modern ? 24 : 22
    static let highlightRadius: CGFloat = modern ? 7 : 4
}
