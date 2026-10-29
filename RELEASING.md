# Releasing

```sh
./release.sh 0.2.0 "What changed"
```

Bumps the version for both apps, signs with Developer ID, notarizes, and publishes `Snowhush.dmg`, `appcast.xml` and `Snowhush-Setup.exe` as a GitHub release. The Mac app checks the appcast of the latest release with Sparkle; the Windows app asks GitHub for the latest release and offers its `Snowhush-Setup.exe`.

One-time setup on the release machine:

- a "Developer ID Application" certificate in the keychain;
- a notarytool profile: `xcrun notarytool store-credentials snowhush --apple-id <apple-id> --team-id <team-id>`;
- the Sparkle EdDSA private key in the login keychain (`.build/sparkle-tools/bin/generate_keys`); its public key is `SUPublicEDKey` in `Resources/Info.plist`;
- for Windows: `rustup target add x86_64-pc-windows-msvc` and `cargo install cargo-xwin`.

## Windows

`windows/` is the Windows app in Rust, cross-compiled from the Mac. Its snow comes from `core/`, the simulation without any platform, which `cd core && cargo test` tests natively. `windows/build.sh` builds the app and its installer, which carries the app, into `dist/Snowhush-Setup.exe`. The installer works per user, without administrator rights, and `Snowhush.exe --uninstall` removes it again. The installer is unsigned, so SmartScreen may warn before running it.

## Checks

Every pull request and every push to `main` runs `.github/workflows/ci.yml` on GitHub: formatting and the simulation tests on Linux, the Windows app built on Windows, and the Mac app built on macOS.

## Site

`site/` is a static page served by Cloudflare Pages at [snowhush.pages.dev](https://snowhush.pages.dev): no build command, output directory `site`. Every push to `main` deploys it.
