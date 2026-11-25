<p align="center">
  <a href="https://snowhush.app"><img src="site/assets/icon-512.png" width="128" height="128" alt="Snowhush icon"></a>
</p>

<h1 align="center">Snowhush</h1>

<p align="center">It's snowing on your desktop.</p>

<p align="center">
  <a href="https://github.com/mbezruchko/Snowhush/releases/latest/download/Snowhush.dmg"><picture><source media="(prefers-color-scheme: dark)" srcset=".github/readme/macos-dark.svg"><img src=".github/readme/macos-light.svg" width="20" height="16" align="absmiddle" alt=""></picture><b>Download for macOS</b></a>
  &nbsp;·&nbsp;
  <a href="https://apps.microsoft.com/detail/9P00NQMGBKGP"><picture><source media="(prefers-color-scheme: dark)" srcset=".github/readme/windows-dark.svg"><img src=".github/readme/windows-light.svg" width="20" height="16" align="absmiddle" alt=""></picture><b>Download for Windows</b></a>
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

## Snow that remembers your touch (macOS)

Power use adapts automatically: on battery, background snowfall runs at 30 fps with 85% of the usual particles, returning to up to 60 fps during interaction. Low Power Mode or thermal pressure uses 60% of the particles and caps interaction at 30 fps. Max Performance only overrides the frame rate on external power. Window tracking polls less frequently while idle on battery, while drags and Space transitions retain fast tracking. Static snow no longer triggers GPU redraws for glints. Two frames in flight reduce drawable and staging-buffer memory; hidden/sleeping overlays release staging buffers and optional artwork, and oversized buffers shrink periodically after activity subsides.

Flakes and loose powder share smoothly changing wind across displays. Gusts lift fresh powder more easily than compacted snow. Window ledges hold small rounded snow lips that eventually break into falling lumps, returning their snow to the scene. Thin powder melts faster and dense, worked snow lasts longer. Soft shading adds depth to hollows, with stationary crystalline highlights instead of flickering drifts. Snowman animations also reduce their update rate in power-saving modes.

Slow strokes leave a groove and push snow into banks. Repeated strokes compact the surface: worked snow resists a gentle pass and holds its shape longer, while a fast flick can break it into clumps that burst when they land. Rolling snowballs also presses the snow left beneath them.

Fresh snowfall gradually buries the compacted surface, softens the banks and fills the grooves. This follows the snow that actually lands, so turning snowfall off does not instantly reset your tracks. Melting and shaking still affect the drifts normally. Surface compaction changes how snow moves, without subtracting snow or compacting autumn leaves and petals.

## A shared winter scene (macOS)

Snowballs collide and push each other; a hard throw can knock the upper ball off a snowman. Figures press rounded footprints into settled snow according to the bottom sphere's curvature and the weight it carries. Equal weight spreads over a larger ball; stacking adds pressure to the bottom ball. Snow gradually firms under the load and stops further sinking. Displaced snow banks up beside the footprint instead of disappearing, and leaves remain in place.

Falling flakes can settle on the figures themselves. Their heads and shoulders become lightly dusted, and moving or hitting them shakes that snow back into the scene. The sculpting hint retires after the first successful roll. Trackpad feedback marks contact and assembly; optional **Settings › Snow interaction sounds** adds a quiet synthesized crunch (off by default).

## Sculpt the snow (macOS)

Hold **Option (⌥)** over a settled drift, then **click and drag with the trackpad or mouse**. The ball gathers the actual snow under your stroke, leaving a groove. Let go to put it down. Roll a smaller ball, lift it to the top of a larger one, and release when the snap outline appears. A gentle magnetic pull helps align the ball with the nearest suitable stack; three stacked balls become a snowman. Trackpads give a small haptic tick at the snap point.

**Shift-drag** any sphere in a stack to lift that sphere out and reuse it elsewhere; Escape restores the stack if you change your mind. A Shift-click without dragging leaves it intact. Drag existing balls and snowmen normally to pick them up; a quick release throws them. Hold still briefly before releasing to place a figure without throwing it. Loose balls fall, bounce and roll to rest on the ground or a window ledge. Figures follow the supporting window and fall when it closes. Hold Option while rolling a loose ball through a drift to gather more snow. A plain click breaks the sphere you aim at and returns its snow to the scene. A completed snowman greets you, watches the cursor, and blinks; reduced-motion settings suppress these decorative movements. Option-double-click a finished snowman to change its scarf. Option-secondary-click (a two-finger trackpad click) crumbles it back into snow. Escape cancels the current stroke and returns its snow. Release Option to return to normal desktop interaction. Figures are saved per display, with up to 24 per display. Moving figures are checkpointed periodically, and the scene is saved immediately when the display sleeps, disconnects, or the app quits. Sculpting works over visible snow in front of windows; it does not capture clicks with Behind Windows enabled.

## Autumn Leaves

Choose Mode › Autumn Leaves and leaves fall instead of snow. They turn and swing on the way down, settle in heaps you can see are made of leaves, scatter under the cursor and come off a window you shake or drag. Melt Snow and Clear Snow become Wither Leaves and Clear Leaves.

## Rain

Choose Mode › Rain and rain beats down instead of snow, splashing where it lands. On the tops of your windows, the Dock and the taskbar it spatters the glass with spray and sets drops running down it like beads of water on a window: they catch on specks, gather the spray they meet and lurch on, leaving a wet film and a trail of droplets. Others hang under bottom edges until they grow heavy enough to let go. Drag the cursor through them to smear them, or shake or drag a window to throw them off. Rain doesn't pile up, so Melt Snow and Clear Snow step aside, and so does Ground under Accumulate: rain always splashes along the bottom.

## Cherry Blossom

Choose Mode › Cherry Blossom and pink petals fall instead of snow. Curved like a spoon, they glint as they tumble and glow where the light comes through them, and now and then a whole flower on its stalk spins down. They heap up and scatter just as leaves do, and with Wilt Petals on, the fallen ones brown from their edges before they go. Melt Snow and Clear Snow become Wilt Petals and Clear Petals.

## Halloween

From October 30 until dawn on November 1 the snow turns spooky: jack-o'-lanterns glow in the drift and ghosts pop up out of it with signs. Choose Mode › Halloween to have it any other day, as long as snow is falling.

Everything else lives in the menu bar: an Intensity slider for how much falls, Opacity for how see-through it is, a row showing its value that folds out a slider (on Linux and in the plain Windows menu, both are submenus of steps), Accumulate, which folds out a row of icons for what the snow settles on (the ground, the Dock, windows and desktop widgets on the Mac; the ground and windows on Windows; the ground, a floating panel and windows on Linux, as a submenu), Mode (Snow, Autumn Leaves, Rain, Cherry Blossom, and Halloween on top of snow) and Clear Snow, with the rest under Settings: Melt Snow, Deep Interaction (on Windows and the Mac, the snow also settles on a selection dragged across the desktop), Behind Windows, Show over Full Screen, Show in Screenshots, Max Performance (draws every frame at the display's full rate) and Open at Login. On Windows and the Mac, Settings stays open while you switch its options. On Windows, Behind Taskbar lets the snow settle at the bottom of the screen under a see-through taskbar, such as one made clear with TranslucentTB.

## Building

```sh
./build.sh            # build/Snowhush.app, signed ad hoc
./build.sh --native   # this Mac only; works with Command Line Tools
open build/Snowhush.app
windows/build.sh      # dist/Snowhush-Setup.exe, cross-compiled from macOS
```

Run `scripts/test-snowmen.sh` on macOS to check snow collection, stacking, cancellation, persistence, compaction, wind, melting and mass conservation using in-memory preferences. Run `scripts/test-snow-scene.sh` for a synthetic two-display simulation benchmark, window transfer and resize checks, production Metal pipeline compilation, and an offscreen snow render (saved to `/tmp/snowhush-snow-preview.png`). These checks do not replace testing physical display connections or system sleep.

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
