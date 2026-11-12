<p align="center">
  <a href="https://snowhush.app"><img src="site/assets/icon-512.png" width="128" height="128" alt="Snowhush icon"></a>
</p>

<h1 align="center">Snowhush</h1>

<p align="center">It's snowing on your desktop.</p>

<p align="center">
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush.dmg"><picture><source media="(prefers-color-scheme: dark)" srcset=".github/readme/macos-dark.svg"><img src=".github/readme/macos-light.svg" width="20" height="16" align="absmiddle" alt=""></picture><b>Download for macOS</b></a>
  &nbsp;·&nbsp;
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush-Setup.exe"><picture><source media="(prefers-color-scheme: dark)" srcset=".github/readme/windows-dark.svg"><img src=".github/readme/windows-light.svg" width="20" height="16" align="absmiddle" alt=""></picture><b>Download for Windows</b></a>
  &nbsp;·&nbsp;
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush-x86_64.AppImage"><picture><source media="(prefers-color-scheme: dark)" srcset=".github/readme/linux-dark.svg"><img src=".github/readme/linux-light.svg" width="20" height="16" align="absmiddle" alt=""></picture><b>Download for Linux</b></a>
  &nbsp;·&nbsp;
  <a href="https://snowhush.app"><picture><source media="(prefers-color-scheme: dark)" srcset=".github/readme/link-dark.svg"><img src=".github/readme/link-light.svg" width="20" height="16" align="absmiddle" alt=""></picture>snowhush.app</a>
  <br>
  <sub>macOS 14 or later · Windows 10 or 11 · Linux with KDE Plasma 6, Hyprland, Sway or niri · free, source available</sub>
</p>

https://github.com/user-attachments/assets/291c3cd5-3563-4b86-8b66-dfd924f980fc

A tiny app that lives in the menu bar on macOS and in the tray on Windows and Linux. Soft snow drifts across your screen, settles along the tops of your windows and slips off when you drag them. Sweep the drift aside with your cursor.

## What the snow does

- **Falls.** Every flake keeps its own pace and sways in a slow breeze.
- **Settles.** It piles up and slumps to a natural slope along the bottom of the screen, on any window with room above it, and on the Dock.
- **Moves.** Push the cursor through a drift and it parts and sprays. Drag or shake a window and its snow slides off and lands below.
- **Drops.** Close or minimize a window and its snow drops at once, in lumps that burst into powder where they land.
- **Melts.** With Melt Snow on, the drift thaws slowly, so it never takes over your screen.
- **Hides.** Screenshots and screen sharing show your desktop without it, unless you turn on Show in Screenshots.
- **Rests.** About 6% of one processor core and 65 MB of memory, and less on battery or in Low Power Mode. It pauses while your screen sleeps, locks or is covered.

## Autumn Leaves

Choose Mode › Autumn Leaves and leaves fall instead of snow. They turn and swing on the way down, settle in heaps you can see are made of leaves, scatter under the cursor and come off a window you shake or drag. Melt Snow and Clear Snow become Wither Leaves and Clear Leaves.

## Rain

Choose Mode › Rain and rain beats down instead of snow, splashing where it lands. On the tops of your windows, the Dock and the taskbar it spatters the glass with spray and sets drops running down it like beads of water on a window: they catch on specks, gather the spray they meet and lurch on, leaving a wet film and a trail of droplets. Others hang under bottom edges until they grow heavy enough to let go. Drag the cursor through them to smear them, or shake or drag a window to throw them off. Rain doesn't pile up, so Melt Snow and Clear Snow step aside, and so does Ground under Accumulate: rain always splashes along the bottom.

## Cherry Blossom

Choose Mode › Cherry Blossom and pink petals fall instead of snow. Curved like a spoon, they glint as they tumble and glow where the light comes through them, and now and then a whole flower on its stalk spins down. They heap up and scatter just as leaves do, and with Wilt Petals on, the fallen ones brown from their edges before they go. Melt Snow and Clear Snow become Wilt Petals and Clear Petals.

## Halloween

From October 30 until dawn on November 1 the snow turns spooky: jack-o'-lanterns glow in the drift and ghosts pop up out of it with signs. Choose Mode › Halloween to have it any other day, as long as snow is falling.

Everything else lives in the menu bar: an Intensity slider for how much falls, Opacity for how see-through it is, a row showing its value that folds out a slider (on Linux and in the plain Windows menu, both are submenus of steps), Accumulate, which folds out a row of icons for what the snow settles on (the ground, the Dock, windows and desktop widgets on the Mac; the ground and windows on Windows; the ground, a floating panel and windows on Linux, as a submenu), Mode (Snow, Autumn Leaves, Rain, Cherry Blossom, and Halloween on top of snow) and Clear Snow, with the rest under Settings: Melt Snow, Deep Interaction (on Windows and the Mac, the snow also settles on a selection dragged across the desktop), Behind Windows, Show over Full Screen, Show in Screenshots, Max Performance (draws every frame at the display's full rate) and Open at Login. On Windows and the Mac, Settings stays open while you switch its options. On Windows, Behind Taskbar lets the snow settle at the bottom of the screen under a see-through taskbar, such as one made clear with TranslucentTB.

## Weather sync (macOS)

Choose Mode › Weather Effects, then open Weather Settings to choose what you want to see (or press ⌘, while Snowhush is active). Each effect has three settings: Off, Weather, and Always. Weather follows local conditions; Always works independently of them, even when location access or the network is unavailable. Snow, rain, clouds, fog, sunlight, lightning, and moonlight with stars can all be controlled separately.

Presets include All Follow Weather, Only Snow When It Snows, Fog Only, and All Effects Off. Only Snow When It Snows shows snow during a snowfall and nothing during rain, clear skies, fog, or thunderstorms. Fog Only turns on fog immediately with every other effect off. You can also combine rules, such as permanent fog with snow only when it snows. Choices are saved between launches.

When at least one effect follows the weather, allow location access when macOS asks, or choose a city in Weather Settings. When every effect is Off or Always, no location fixes or weather requests are made. Turning Weather Effects off or selecting a manual particle mode brings back your original manual settings.

Rain and snow follow the current conditions, including mixed precipitation; clouds drift overhead, fog rolls across the lower screen, sunlight glows on clear days, and thunderstorms produce short branching strikes with local cloud light. The first strike appears shortly after enabling lightning, with varied pauses between later strikes. Clear nights have a small shaded moon with a close halo and scattered stars. Forcing the moon on during the day leaves the other effects' lighting alone. The Intensity slider controls the strength, and Off stops the effects. Reduce Motion keeps the atmospheric layers still and disables lightning.

Weather comes from [Open-Meteo](https://open-meteo.com/), independently of Apple's Weather app, and refreshes every 15 minutes while the Mac is awake. Location is requested at city accuracy; only coordinates rounded to two decimal places are sent to the weather service, and automatic locations are not saved. If location access is unavailable, choose a city instead. A network failure keeps a recent reading; readings older than an hour stop driving the effects until a refresh succeeds.

## Building

```sh
./build.sh            # build/Snowhush.app, signed ad hoc
open build/Snowhush.app
windows/build.sh      # dist/Snowhush-Setup.exe, cross-compiled from macOS
```

### Linux

`linux/` is a native version for Wayland: KDE Plasma 6, Hyprland, Sway and niri. It shares the snow's physics with the Windows version and draws with OpenGL ES in a layer-shell overlay. Wayland doesn't tell apps where other windows are or where the pointer is, so each compositor is asked in its own way: on KDE, Snowhush loads a small KWin script (`linux/src/snowhush.js`) while it runs, which reports both; Hyprland answers over its IPC socket, which tells both too; Sway and niri answer over theirs, though they don't tell where the pointer is, and niri only where floating windows are, so on niri only those collect snow.

Each release carries an AppImage that runs on any distribution, a `.deb` and an `.rpm`. On Arch, `makepkg -si` in `linux/packaging/aur` installs the release's build as a package. To build it yourself:

```sh
cd linux && cargo build --release   # linux/target/release/snowhush
cd linux/packaging && makepkg -si   # Arch: builds and installs it with its icon and menu entry
```

`linux/bench/run.sh sway|niri|kwin` tries it in a compositor inside a container, without a display.

Everything lives in the tray, as on Windows. Settings go to `~/.config/snowhush/settings`, Open at Login to `~/.config/autostart/snowhush.desktop`, the log to `~/.local/state/snowhush/snowhush.log`.

## License

[PolyForm Noncommercial 1.0.0](LICENSE): use, share and change Snowhush freely, keeping its copyright notice with it. Selling it, or anything built on its code, isn't allowed. Versions up to 0.6.1 were released under the MIT License.
