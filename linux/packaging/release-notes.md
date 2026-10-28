A preview of Snowhush for Linux, from the `kde-port` branch of this fork. It isn't part of the official Snowhush releases yet: it's offered upstream in [mbezruchko/Snowhush#1](https://github.com/mbezruchko/Snowhush/issues/1).

**Needs KDE Plasma 6 on Wayland**, on x86_64. It doesn't run on GNOME or on X11.

### Install

- **Arch, CachyOS, Manjaro:** `sudo pacman -U snowhush-0.5.6-1-x86_64.pkg.tar.zst`
- **Fedora, openSUSE:** `sudo dnf install ./snowhush-0.5.6-1.x86_64.rpm` (on openSUSE, `sudo zypper install ./snowhush-0.5.6-1.x86_64.rpm`)
- **Anything else:** `chmod +x Snowhush-0.5.6-x86_64.AppImage && ./Snowhush-0.5.6-x86_64.AppImage`

Then start Snowhush from the app menu. It lives in the system tray, where you change the snowfall, let the drift melt, keep the snow off your windows, clear it, or turn on Open at Login.

### What it does

Snow falls over your desktop and settles along the tops of your windows and on the panel, following their rounded corners. Sweep it aside with the cursor, drag or shake a window and its snow slides off, close or minimize one and its snow drops. It stays out of screenshots and screen sharing unless you turn on Show in Screenshots.

Wayland doesn't tell apps where other windows or the pointer are, so Snowhush loads a small KWin script while it runs and unloads it when it quits.

### Not in this version

- The welcome card on first launch: the snow just starts.
- The update check. Download new versions from this page.

Tested on CachyOS with Plasma 6.8.1, two monitors and an NVIDIA GPU; the packages were also installed and started on Fedora, Ubuntu 24.04 and Arch. `SHA256SUMS` has the checksums.
