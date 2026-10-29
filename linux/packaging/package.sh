#!/usr/bin/env bash
# Packages a built Linux app for a release: an AppImage, a .deb, an .rpm, and the tarball the AUR package is made
# from. Their names leave the version out, so the site can link to the latest release's.
#   linux/packaging/package.sh <binary> <output folder>
# Needs dpkg-deb, rpmbuild and curl. The AppImage tools are downloaded and checked against their checksums.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
binary=$(realpath "$1")
mkdir -p "$2"
out=$(cd "$2" && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/linux/Cargo.toml" | head -1)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

appimagetool=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
appimagetool_sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
runtime=https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
runtime_sha256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d

# What every package installs, where it goes.
stage=$work/stage
install -Dm755 "$binary" "$stage/usr/bin/snowhush"
install -Dm644 "$here/snowhush.desktop" "$stage/usr/share/applications/snowhush.desktop"
install -Dm644 "$root/site/assets/icon-64.png" "$stage/usr/share/icons/hicolor/64x64/apps/snowhush.png"
install -Dm644 "$root/windows/assets/icon-256.png" "$stage/usr/share/icons/hicolor/256x256/apps/snowhush.png"
install -Dm644 "$root/site/assets/icon-512.png" "$stage/usr/share/icons/hicolor/512x512/apps/snowhush.png"
install -Dm644 "$root/LICENSE" "$stage/usr/share/licenses/snowhush/LICENSE"

echo "Tarball…"
tar -C "$stage" --owner=0 --group=0 -czf "$out/snowhush-linux-x86_64.tar.gz" usr

echo "Debian package…"
deb=$work/deb
cp -a "$stage" "$deb"
mkdir -p "$deb/DEBIAN" "$deb/usr/share/doc/snowhush"
mv "$deb/usr/share/licenses/snowhush/LICENSE" "$deb/usr/share/doc/snowhush/copyright"
rm -r "$deb/usr/share/licenses"
cat > "$deb/DEBIAN/control" <<EOF
Package: snowhush
Version: $version
Architecture: amd64
Maintainer: Mykhailo Bezruchko <michael.bezruchko@gmail.com>
Depends: libc6 (>= 2.35), libgcc-s1, libwayland-client0, libwayland-egl1, libegl1
Section: x11
Priority: optional
Homepage: https://snowhush.pages.dev
Description: Snow on your Wayland desktop
 Soft snow drifts across your desktop, settles on the tops of your windows,
 slides off when you drag them and parts around the cursor. Needs KDE Plasma 6,
 Sway or niri on Wayland.
EOF
dpkg-deb --root-owner-group -Zxz --build "$deb" "$out/snowhush-x86_64.deb" > /dev/null

echo "RPM…"
rpmbuild -bb --quiet "$here/snowhush.spec" \
    --define "snowhush_version $version" \
    --define "snowhush_stage $stage" \
    --define "_rpmdir $work/rpm" \
    --define "_topdir $work/rpmbuild"
mv "$work"/rpm/x86_64/snowhush-*.rpm "$out/snowhush-x86_64.rpm"

echo "AppImage…"
fetch() {
    curl -fsSL -o "$1" "$2"
    echo "$3  $1" | sha256sum --check --quiet
}
fetch "$work/appimagetool" "$appimagetool" "$appimagetool_sha256"
fetch "$work/runtime" "$runtime" "$runtime_sha256"
chmod +x "$work/appimagetool"
app=$work/AppDir
cp -a "$stage" "$app"
cp "$here/snowhush.desktop" "$app/snowhush.desktop"
cp "$stage/usr/share/icons/hicolor/256x256/apps/snowhush.png" "$app/snowhush.png"
cat > "$app/AppRun" <<'EOF'
#!/bin/sh
exec "$(dirname "$(readlink -f "$0")")/usr/bin/snowhush" "$@"
EOF
chmod +x "$app/AppRun"
# Unpacked rather than mounted, since a build machine needn't have FUSE.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$work/appimagetool" --no-appstream --runtime-file "$work/runtime" \
    "$app" "$out/Snowhush-x86_64.AppImage" > /dev/null 2>&1

(cd "$out" && sha256sum snowhush-linux-x86_64.tar.gz snowhush-x86_64.deb snowhush-x86_64.rpm Snowhush-x86_64.AppImage > SHA256SUMS-linux)
ls -l "$out"
