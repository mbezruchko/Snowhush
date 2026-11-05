import AppKit

/// The screenshot tool, picking a window to capture after ⌘⇧4 and Space or in ⌘⇧5's window mode, takes the window
/// drawn under the pointer, and to it the overlay, drawn with Metal, is drawn all over: it picks the snow instead of
/// the window under it, and with the snow kept out of screenshots, captures nothing. So the overlays step aside while
/// it is up. SystemUIServer starts `screencapture` for each screenshot shortcut, and its fork is caught as it happens.
final class ScreenshotWatch {
    /// Whether the overlays should be out of the screenshot tool's way.
    var stepAside: (Bool) -> Void = { _ in }

    private var server: DispatchSourceProcess?
    private var capture: DispatchSourceProcess?
    private var lookout: Timer?

    func start() {
        guard let pid = NSWorkspace.shared.runningApplications.first(where: { $0.bundleIdentifier == "com.apple.systemuiserver" })?
            .processIdentifier
        else {
            retry()
            return
        }
        let source = DispatchSource.makeProcessSource(identifier: pid, eventMask: [.fork, .exit], queue: .main)
        source.setEventHandler { [weak self] in
            guard let self, let server else { return }
            if server.data.contains(.exit) {
                server.cancel()
                self.server = nil
                retry()
            } else {
                forked(from: pid)
            }
        }
        source.resume()
        server = source
    }

    /// SystemUIServer comes back by itself when it quits, and the watch with it.
    private func retry() {
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in self?.start() }
    }

    private func forked(from server: pid_t) {
        guard capture == nil else { return }
        var children = [pid_t](repeating: 0, count: 64)
        let count = proc_listchildpids(server, &children, Int32(children.count * MemoryLayout<pid_t>.size))
        guard let pid = children.prefix(Int(max(count, 0))).first(where: Self.picksWindows) else { return }
        let source = DispatchSource.makeProcessSource(identifier: pid, eventMask: .exit, queue: .main)
        source.setEventHandler { [weak self] in self?.end() }
        source.resume()
        capture = source
        stepAside(true)
        if kill(pid, 0) != 0 {
            end()
            return
        }
        if Preferences.showInScreenshots { watchForSelection() }
    }

    /// Shown in screenshots, the snow comes back as soon as a region is dragged out, so the region takes it in: the
    /// tool's selection then lies over everything, the pointer included.
    private func watchForSelection() {
        let cursor = Int(CGWindowLevelForKey(.cursorWindow))
        let timer = Timer(timeInterval: 0.05, repeats: true) { [weak self] _ in
            guard let self else { return }
            let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
            let selecting = list.contains {
                $0[kCGWindowOwnerName as String] as? String == "screencapture"
                    && ($0[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0 > cursor
            }
            guard selecting else { return }
            lookout?.invalidate()
            lookout = nil
            stepAside(false)
        }
        timer.tolerance = 0.01
        RunLoop.main.add(timer, forMode: .common)
        lookout = timer
    }

    private func end() {
        capture?.cancel()
        capture = nil
        lookout?.invalidate()
        lookout = nil
        stepAside(false)
    }

    /// `screencapture` run for ⌘⇧4 (-i, interactive) or ⌘⇧5 (-U, its toolbar); for ⌘⇧3 it takes the screen as it is.
    private static func picksWindows(_ pid: pid_t) -> Bool {
        var path = [CChar](repeating: 0, count: 4096)
        guard proc_pidpath(pid, &path, UInt32(path.count)) > 0, String(cString: path) == "/usr/sbin/screencapture" else { return false }
        return arguments(of: pid).dropFirst().contains { argument in
            argument.hasPrefix("-") && !argument.hasPrefix("--") && argument.dropFirst().contains { $0 == "i" || $0 == "U" }
        }
    }

    /// A process's arguments, from the kernel: its argument count, its path, then the arguments, each ending in a zero.
    private static func arguments(of pid: pid_t) -> [String] {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > MemoryLayout<Int32>.size else { return [] }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0 else { return [] }
        let count = Int(buffer.withUnsafeBytes { $0.load(as: Int32.self) })
        var start = MemoryLayout<Int32>.size
        while start < size, buffer[start] != 0 { start += 1 }
        while start < size, buffer[start] == 0 { start += 1 }
        var arguments: [String] = []
        while start < size, arguments.count < count {
            var end = start
            while end < size, buffer[end] != 0 { end += 1 }
            arguments.append(String(decoding: buffer[start..<end], as: UTF8.self))
            start = end + 1
        }
        return arguments
    }
}
