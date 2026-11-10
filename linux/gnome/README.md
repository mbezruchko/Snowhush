# GNOME Wayland

Native support for [upstream issue #100](https://github.com/mbezruchko/Snowhush/issues/100).
The existing Rust application, shared `snowhush-core` physics, EGL renderer and GLSL assets are reused.
There is no JavaScript simulation, XWayland overlay, layer-shell dependency on GNOME, or AppIndicator dependency.

GNOME does not implement layer-shell. The Shell extension launches the Rust binary as a trusted
`Meta.WaylandClient`, manages its transparent xdg-shell windows, and reports windows, work areas and
pointer coordinates over the session bus. An empty Wayland input region passes clicks and scrolls
through the overlay. Every monitor has its own simulation and surface. The Shell panel provides
start/stop, intensity, opacity, all four weather modes, Halloween, accumulation, melting and layering.
Settings use the existing `~/.config/snowhush/settings` file. Logs use
`~/.local/state/snowhush/snowhush.log` (or the corresponding XDG directories).

## Build and install

GNOME Shell **46**, on Wayland, is the currently tested target. Other Shell versions must be tested
before adding them to `metadata.json`. Rust edition 2024 and Wayland development libraries are needed:

```sh
sudo apt install build-essential pkg-config libwayland-dev
cargo build --locked --release --manifest-path linux/Cargo.toml
linux/gnome/install.sh
gnome-extensions enable snowhush@mbezruchko.github.io
```

After installing a new extension, GNOME may require logging out and back in to discover it.
The extension starts the engine when enabled, including after logging in. Clicking Snowhush in the
applications menu starts it through the extension; running the regular binary does the same on GNOME.
Disabling the extension sends SIGTERM to the Rust event loop and releases all surfaces.

To remove it:

```sh
gnome-extensions disable snowhush@mbezruchko.github.io
rm -r ~/.local/share/gnome-shell/extensions/snowhush@mbezruchko.github.io
rm ~/.local/share/applications/snowhush.desktop
```

`--gnome-engine` is an internal argument for the trusted subprocess; ordinary launches should omit it.
The existing layer-shell backends remain in use for KDE, Hyprland, Sway and niri.
GNOME screenshots and screen sharing include the snow: the screenshot-exclusion switch is not offered.
The panel UI currently uses English and selects one weather mode at a time. Floating-panel detection
is not implemented; ground follows the work area GNOME reports.

## Validation

```sh
cargo test --locked --manifest-path core/Cargo.toml
cargo test --locked --manifest-path linux/Cargo.toml
cargo fmt --check --manifest-path linux/Cargo.toml
linux/bench/gnome.sh
```

The GNOME bench uses a separate D-Bus session, isolated XDG directories, a headless Mutter monitor
and software rendering. It does not change the current desktop or its enabled extensions.

The trusted-client approach is also used by [wojtipek's GNOME port](https://github.com/wojtipek/snowhush-gnome),
discussed in [issue #15](https://github.com/mbezruchko/Snowhush/issues/15). This implementation integrates
with upstream's current Linux application and shared core instead of maintaining a separate engine.
