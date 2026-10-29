# Releasing

```sh
./release.sh 0.2.0 "What changed"
```

Bumps the version for all three apps, signs with Developer ID, notarizes, and publishes `Snowhush.dmg`, `appcast.xml` and `Snowhush-Setup.exe` as a GitHub release. The Mac app checks the appcast of the latest release with Sparkle; the Windows app asks GitHub for the latest release and offers its `Snowhush-Setup.exe`. Once the release is published, GitHub Actions adds the Linux packages to it a few minutes later (`.github/workflows/release.yml`).

One-time setup on the release machine:

- a "Developer ID Application" certificate in the keychain;
- a notarytool profile: `xcrun notarytool store-credentials snowhush --apple-id <apple-id> --team-id <team-id>`;
- the Sparkle EdDSA private key in the login keychain (`.build/sparkle-tools/bin/generate_keys`); its public key is `SUPublicEDKey` in `Resources/Info.plist`;
- for Windows: `rustup target add x86_64-pc-windows-msvc` and `cargo install cargo-xwin`.

## Windows

`windows/` is the Windows app in Rust, cross-compiled from the Mac. Its snow comes from `core/`, the simulation without any platform, which `cd core && cargo test` tests natively. `windows/build.sh` builds the app and its installer, which carries the app, into `dist/Snowhush-Setup.exe`. The installer works per user, without administrator rights, and `Snowhush.exe --uninstall` removes it again. The installer is unsigned, so SmartScreen may warn before running it.

## Linux

`linux/` is the Linux app in Rust, on the same `core/`, for KDE Plasma 6, Sway and niri on Wayland. Macs can't build it, so GitHub does: `linux/packaging/package.sh` turns the binary built on Ubuntu 22.04 into `Snowhush-x86_64.AppImage`, `snowhush-x86_64.deb`, `snowhush-x86_64.rpm` and `snowhush-linux-x86_64.tar.gz`, the last for the AUR. A release without them, such as one made before, gets them from the workflow's "Run workflow" button with its tag.

After the packages are up, bring the AUR along: `linux/packaging/aur/update.sh <version>` sets the version and checksum in `linux/packaging/aur/PKGBUILD` and writes its `.SRCINFO`; copy both into the clone of `ssh://aur@aur.archlinux.org/snowhush-bin.git`, commit and push.

## Checks

Every pull request and every push to `main` runs `.github/workflows/ci.yml` on GitHub: formatting and the simulation tests on Linux, the Linux app built on Ubuntu 22.04 and kept as a download for testing, the Windows app built on Windows, and the Mac app built on macOS.

## Site

`site/` is a static page served by Cloudflare Pages at [snowhush.pages.dev](https://snowhush.pages.dev): no build command, output directory `site`. Every push to `main` deploys it.
