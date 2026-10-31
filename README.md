<p align="center">
  <img src="site/assets/icon-512.png" width="128" height="128" alt="Snowhush icon">
</p>

<h1 align="center">Snowhush</h1>

<p align="center">It's snowing on your desktop.</p>

<p align="center">
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush.dmg"><b>Download for macOS</b></a>
  &nbsp;·&nbsp;
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush-Setup.exe"><b>Download for Windows</b></a>
  &nbsp;·&nbsp;
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush-x86_64.AppImage"><b>Download for Linux</b></a>
  &nbsp;·&nbsp;
  <a href="https://snowhush.pages.dev">snowhush.pages.dev</a>
  <br>
  <sub>macOS 14 or later · Windows 10 or 11 · Linux with KDE Plasma 6, Sway or niri · free and open source</sub>
</p>

https://github.com/user-attachments/assets/291c3cd5-3563-4b86-8b66-dfd924f980fc

A tiny app that lives in the menu bar on macOS and in the tray on Windows and Linux. Soft snow drifts across your screen, settles along the tops of your windows and slips off when you drag them. Sweep the drift aside with your cursor.

## What the snow does

- **Falls.** Every flake keeps its own pace and sways in a slow breeze. Variable lets the snowfall rise and ease on its own over the day.
- **Settles.** It piles up and slumps to a natural slope along the bottom of the screen, on any window with room above it, and on the Dock.
- **Moves.** Push the cursor through a drift and it parts and sprays. Drag or shake a window and its snow slides off and lands below.
- **Drops.** Close or minimize a window and its snow drops at once, in lumps that burst into powder where they land.
- **Melts.** With Melt Snow on, the drift thaws slowly, so it never takes over your screen.
- **Hides.** Screenshots and screen sharing show your desktop without it, unless you turn on Show in Screenshots.
- **Rests.** About 6% of one processor core and 65 MB of memory, and less on battery or in Low Power Mode. It pauses while your screen sleeps, locks or is covered.

Everything else lives in the menu bar: Snowfall (Off, Light, Steady, Heavy, Variable), Accumulate, Melt Snow, Snow on Windows, Clear Snow, Behind Windows, Show in Screenshots and Open at Login. On Windows, Behind Taskbar lets the snow settle at the bottom of the screen under a see-through taskbar, such as one made clear with TranslucentTB.

## Building

```sh
./build.sh            # build/Snowhush.app, signed ad hoc
open build/Snowhush.app
windows/build.sh      # dist/Snowhush-Setup.exe, cross-compiled from macOS
```

### Linux

`linux/` is a native version for Wayland: KDE Plasma 6, Sway and niri. It shares the snow's physics with the Windows version and draws with OpenGL ES in a layer-shell overlay. Wayland doesn't tell apps where other windows are or where the pointer is, so each compositor is asked in its own way: on KDE, Snowhush loads a small KWin script (`linux/src/snowhush.js`) while it runs, which reports both; Sway and niri answer over their IPC sockets, though they don't tell where the pointer is, and niri only where floating windows are, so on niri only those collect snow.

Each release carries an AppImage that runs on any distribution, a `.deb` and an `.rpm`. On Arch, `makepkg -si` in `linux/packaging/aur` installs the release's build as a package. To build it yourself:

```sh
cd linux && cargo build --release   # linux/target/release/snowhush
cd linux/packaging && makepkg -si   # Arch: builds and installs it with its icon and menu entry
```

`linux/bench/run.sh sway|niri|kwin` tries it in a compositor inside a container, without a display.

Everything lives in the tray, as on Windows. Settings go to `~/.config/snowhush/settings`, Open at Login to `~/.config/autostart/snowhush.desktop`, the log to `~/.local/state/snowhush/snowhush.log`.

## License

[MIT](LICENSE)
