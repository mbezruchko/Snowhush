import Foundation

/// What falls from the sky. Snow is the default; the others follow the seasons, or the pick in the menu.
enum Theme: Int, CaseIterable {
    case snow, leaves, blossom, candy, drops

    var title: String {
        switch self {
        case .snow: "Snow"
        case .leaves: "Leaves"
        case .blossom: "Blossom"
        case .candy: "Candy"
        case .drops: "Drops"
        }
    }

    /// A drop of water runs off instead of piling up, so nothing gathers while it is set.
    var accumulates: Bool { self != .drops }

    /// The silhouette the renderer draws for a particle of this theme.
    var shape: Float {
        switch self {
        case .snow: 0
        case .leaves: 1
        case .blossom: 2
        case .candy: 3
        case .drops: 4
        }
    }

    /// A short label for the menu item that chooses this theme.
    var symbolName: String {
        switch self {
        case .snow: "snowflake"
        case .leaves: "leaf.fill"
        case .blossom: "camera.macro"
        case .candy: "birthday.cake.fill"
        case .drops: "drop.fill"
        }
    }

    /// The theme that suits the month, for the "Auto" pick.
    static func seasonal(on date: Date = Date(), calendar: Calendar = .current) -> Theme {
        switch calendar.component(.month, from: date) {
        case 3, 4, 5: return .blossom
        case 6, 7, 8: return .drops
        case 9, 10, 11: return .leaves
        default: return .snow
        }
    }
}
