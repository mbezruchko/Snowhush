#!/usr/bin/env bash
# Builds the KDE version's packages into linux/packaging/dist: an Arch package, an RPM and an AppImage,
# all around one binary built in a Debian 12 container, for plain x86-64 and glibc 2.36, so it runs on any
# distribution with Plasma 6 rather than only on the machine that built it.
# Needs: docker, makepkg, rpmbuild. `--publish <tag>` then puts them in a pre-release on GitHub with gh.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/linux/Cargo.toml" | head -1)
dist=$here/dist
stage=$dist/stage
appimagetool_version=1.9.1
rm -rf "$dist/stage" "$dist/AppDir" "$dist"/*.pkg.tar.zst "$dist"/*.rpm "$dist"/*.AppImage
mkdir -p "$dist/target" "$stage" "$dist/tools"

echo "Building snowhush $version…"
docker run --rm \
    -v "$root:/src:ro" \
    -v "$dist/target:/target" \
    -v snowhush-cargo-registry:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target \
    -w /src/linux \
    rust:1-bookworm \
    bash -c "apt-get update -qq && apt-get install -y -qq libwayland-dev >/dev/null \
        && cargo build --release --locked && chown -R $(id -u):$(id -g) /target"
binary=$dist/target/release/snowhush

cp "$binary" "$root/linux/packaging/snowhush.desktop" "$root/LICENSE" "$stage/"
cp "$root/site/assets/icon-64.png" "$root/windows/assets/icon-256.png" "$root/site/assets/icon-512.png" "$stage/"

echo "Arch package…"
(cd "$here" && SNOWHUSH_BINARY=$binary PKGDEST=$dist BUILDDIR=$dist/makepkg makepkg -f --noconfirm >/dev/null)

echo "RPM…"
rpmbuild -bb --quiet "$here/snowhush.spec" \
    --define "snowhush_version $version" \
    --define "_sourcedir $stage" \
    --define "_rpmdir $dist" \
    --define "_build_name_fmt %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm" \
    --define "_topdir $dist/rpmbuild"

echo "AppImage…"
tool=$dist/tools/appimagetool-$appimagetool_version-x86_64.AppImage
if [[ ! -x $tool ]]; then
    curl -fsSL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-x86_64.AppImage"
    chmod +x "$tool"
fi
app=$dist/AppDir
install -Dm755 "$binary" "$app/usr/bin/snowhush"
install -Dm644 "$stage/icon-256.png" "$app/usr/share/icons/hicolor/256x256/apps/snowhush.png"
install -Dm644 "$stage/snowhush.desktop" "$app/usr/share/applications/snowhush.desktop"
cp "$stage/snowhush.desktop" "$stage/icon-256.png" "$app/"
mv "$app/icon-256.png" "$app/snowhush.png"
cat > "$app/AppRun" <<'EOF'
#!/bin/sh
exec "$(dirname "$(readlink -f "$0")")/usr/bin/snowhush" "$@"
EOF
chmod +x "$app/AppRun"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" --no-appstream "$app" "$dist/Snowhush-$version-x86_64.AppImage" >/dev/null 2>&1

rm -rf "$stage" "$app" "$dist/makepkg" "$dist/rpmbuild"
(cd "$dist" && sha256sum ./*.pkg.tar.zst ./*.rpm ./*.AppImage | sed 's| \./| |' > SHA256SUMS)
ls -l "$dist"/*.pkg.tar.zst "$dist"/*.rpm "$dist"/*.AppImage

if [[ ${1:-} == --publish ]]; then
    tag=${2:?usage: release.sh --publish <tag>}
    notes=$here/release-notes.md
    gh release create "$tag" "$dist"/*.pkg.tar.zst "$dist"/*.rpm "$dist"/*.AppImage "$dist/SHA256SUMS" \
        --repo TryCoffee/Snowhush --target "$(git -C "$root" rev-parse HEAD)" --prerelease \
        --title "Snowhush $version for KDE Plasma (preview)" --notes-file "$notes"
fi
