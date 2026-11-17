import AppKit

// Run to read a wallpaper for the app proper: it does only that, and quits.
if CommandLine.arguments.count > 1, CommandLine.arguments[1] == Wallpaper.readArgument {
    Wallpaper.readForAnotherRun(Array(CommandLine.arguments.dropFirst(2)))
    exit(0)
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
