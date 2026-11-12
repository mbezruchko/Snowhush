import AppKit
import DiskArbitration

/// Opened straight from the downloaded disk image, Snowhush copies itself to Applications,
/// ejects the image and starts again from there, so it never runs from a volume that goes away.
enum AppMover {
    /// Returns true when a copy in Applications is about to start and this instance should quit.
    static func moveIfNeeded() -> Bool {
        guard let source = originalBundleURL(), source.path.hasPrefix("/Volumes/"),
              let volume = (try? source.resourceValues(forKeys: [.volumeURLKey]))?.volume,
              isDiskImage(volume)
        else { return false }
        let destination = URL(fileURLWithPath: "/Applications", isDirectory: true).appendingPathComponent(source.lastPathComponent)
        let files = FileManager.default
        do {
            if files.fileExists(atPath: destination.path) {
                try files.trashItem(at: destination, resultingItemURL: nil)
            }
            try files.copyItem(at: source, to: destination)
        } catch {
            NSLog("Snowhush: could not move to Applications: \(error)")
            return false
        }
        // The user already chose to open it; the copy shouldn't ask again.
        run("/usr/bin/xattr", ["-dr", "com.apple.quarantine", destination.path])
        let script = """
        while /bin/kill -0 \(ProcessInfo.processInfo.processIdentifier) 2>/dev/null; do /bin/sleep 0.1; done
        for attempt in 1 2 3 4 5 6 7 8; do /usr/bin/hdiutil detach "$1" -quiet 2>/dev/null && break; /bin/sleep 0.5; done
        /usr/bin/open "$2"
        """
        run("/bin/sh", ["-c", script, "mover", volume.path, destination.path], wait: false)
        return true
    }

    /// An external drive or another volume mounted under /Volumes is left alone.
    private static func isDiskImage(_ volume: URL) -> Bool {
        guard let session = DASessionCreate(kCFAllocatorDefault),
              let disk = DADiskCreateFromVolumePath(kCFAllocatorDefault, session, volume as CFURL),
              let description = DADiskCopyDescription(disk) as? [CFString: Any]
        else { return false }
        return description[kDADiskDescriptionDeviceModelKey] as? String == "Disk Image"
    }

    /// Where the bundle really lives: macOS runs apps opened from a downloaded image from a randomized copy.
    private static func originalBundleURL() -> URL? {
        let url = Bundle.main.bundleURL
        typealias Original = @convention(c) (CFURL, UnsafeMutablePointer<Unmanaged<CFError>?>?) -> Unmanaged<CFURL>?
        guard url.path.contains("/AppTranslocation/"),
              let security = dlopen("/System/Library/Frameworks/Security.framework/Security", RTLD_LAZY),
              let symbol = dlsym(security, "SecTranslocateCreateOriginalPathForURL")
        else { return url }
        let original = unsafeBitCast(symbol, to: Original.self)
        return original(url as CFURL, nil)?.takeRetainedValue() as URL? ?? url
    }

    private static func run(_ path: String, _ arguments: [String], wait: Bool = true) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = arguments
        do {
            try process.run()
            if wait { process.waitUntilExit() }
        } catch {
            NSLog("Snowhush: \(path) failed: \(error)")
        }
    }
}
