import AppKit

/// The displays and the snow on each, for what goes on between them. A window's snow lies on the display showing the
/// window and goes along when the window moves to another. Where Spaces span the displays, a window can lie across
/// two: its snow lies on the one holding most of its top edge, which lends it to the other to draw and to catch the
/// snow falling there, and snow thrown past the edge between them goes on falling on the other side.
final class Displays {
    private struct Display {
        let id: CGDirectDisplayID
        /// In the window list's coordinates: origin at the top left of the primary display.
        let bounds: CGRect
        let uuid: String?
    }

    var snows: [ScreenSnow] = []
    /// Windows can lie across displays, which share their Spaces instead of each having its own.
    private(set) var spanning = false
    private var displays: [Display] = []
    private var spaces: (time: CFTimeInterval, current: [String: UInt64]?) = (-10, nil)

    func update(screens: [NSScreen]) {
        let primaryHeight = screens.first?.frame.height ?? 0
        displays = screens.compactMap { screen in
            guard let id = screen.displayID else { return nil }
            let frame = screen.frame
            let uuid = CGDisplayCreateUUIDFromDisplayID(id).map { CFUUIDCreateString(nil, $0.takeRetainedValue()) as String }
            return Display(id: id, bounds: CGRect(x: frame.minX, y: primaryHeight - frame.maxY, width: frame.width, height: frame.height), uuid: uuid)
        }
        spanning = !NSScreen.screensHaveSeparateSpaces
    }

    /// The display whose snow a window with these bounds, in the window list's coordinates, lies on.
    func owner(of window: CGWindowID, bounds: CGRect, now: CFTimeInterval) -> CGDirectDisplayID? {
        let over = displays.filter { $0.bounds.intersects(bounds) }
        guard over.count > 1 else { return over.first?.id }
        if !spanning, let shown = showing(window, among: over, now: now) { return shown }
        func topEdge(on display: Display) -> CGFloat {
            guard bounds.minY >= display.bounds.minY, bounds.minY < display.bounds.maxY else { return 0 }
            return max(0, min(bounds.maxX, display.bounds.maxX) - max(bounds.minX, display.bounds.minX))
        }
        func area(on display: Display) -> CGFloat {
            let part = bounds.intersection(display.bounds)
            return part.width * part.height
        }
        if let edge = over.max(by: { topEdge(on: $0) < topEdge(on: $1) }), topEdge(on: edge) > 0 { return edge.id }
        return over.max { area(on: $0) < area(on: $1) }?.id
    }

    /// With Spaces of its own on each display, the one whose current Space has the window: only there is it shown.
    private func showing(_ window: CGWindowID, among over: [Display], now: CFTimeInterval) -> CGDirectDisplayID? {
        if now - spaces.time > 0.1 { spaces = (now, Spaces.currentByDisplay()) }
        guard let current = spaces.current, let on = Spaces.of(window) else { return nil }
        return over.first { display in display.uuid.flatMap { current[$0] }.map(on.contains) ?? false }?.id
    }

    /// Moves a window's snow from `snow`'s display to `display`, as when the window is dragged across.
    func hand(_ window: CGWindowID, from snow: ScreenSnow, to display: CGDirectDisplayID) {
        guard let next = snows.first(where: { $0.displayID == display }), next !== snow,
              let carried = snow.simulation.takeWindow(window) else { return }
        next.simulation.adoptWindow(window, carried, shift: shift(from: snow, to: next))
        next.watchWindows()
    }

    /// The snow of windows kept on other displays, where it reaches over `snow`'s display, in its coordinates.
    func borrowed(by snow: ScreenSnow) -> [BorrowedSnow] {
        guard spanning, snows.count > 1 else { return [] }
        let area = CGRect(origin: .zero, size: snow.window.frame.size)
        var lent: [BorrowedSnow] = []
        for other in snows where other !== snow {
            let shift = shift(from: other, to: snow)
            lent += other.simulation.windowIDs.compactMap { other.simulation.lend($0, shift: shift, within: area) }
        }
        return lent
    }

    /// Passes on what went from `snow`'s display to others in its last step: snow that fell on a window's snow lent by
    /// another display goes to that one, and snow thrown past an edge two displays share goes on falling there.
    func pass(from snow: ScreenSnow) {
        for caught in snow.simulation.takeCaught() {
            guard let keeper = snows.first(where: { $0 !== snow && $0.simulation.keeps(caught.id) }) else { continue }
            keeper.simulation.receive(caught, shift: shift(from: snow, to: keeper).x)
        }
        let strays = snow.simulation.takeStrays()
        guard !strays.isEmpty else { return }
        let origin = snow.window.frame.origin
        var passed: [(next: ScreenSnow, strays: [StraySnow])] = []
        for stray in strays {
            let point = CGPoint(x: origin.x + CGFloat(stray.position.x), y: origin.y + CGFloat(stray.position.y))
            guard let next = snows.first(where: { other in
                let frame = other.window.frame
                return other !== snow && point.x >= frame.minX && point.x < frame.maxX && point.y >= frame.minY
            }) else { continue }
            if let index = passed.firstIndex(where: { $0.next === next }) {
                passed[index].strays.append(stray)
            } else {
                passed.append((next, [stray]))
            }
        }
        for (next, strays) in passed { next.simulation.adoptStrays(strays, shift: shift(from: snow, to: next)) }
    }

    /// Takes overlay coordinates on `from`'s display to `to`'s: each starts at its display's bottom left.
    private func shift(from: ScreenSnow, to: ScreenSnow) -> SIMD2<Float> {
        SIMD2(Float(from.window.frame.minX - to.window.frame.minX), Float(from.window.frame.minY - to.window.frame.minY))
    }
}
