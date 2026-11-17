#!/bin/zsh
# Lays out the Microsoft Store package's contents in the folder given (default dist/store): the app, its manifest with
# this version, and the logos, as pack.ps1 packs them into Snowhush.msix on Windows. For a test, the folder can be
# registered as it is (Developer Mode): Add-AppxPackage -Register .\AppxManifest.xml
set -euo pipefail

out=${1:-$(dirname "$0")/../../dist/store}
out=${out:A}
cd "$(dirname "$0")/.."
target=x86_64-pc-windows-msvc

cargo xwin build --release --target $target --bin Snowhush
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
rm -rf "$out"
mkdir -p "$out"
cp target/$target/release/Snowhush.exe "$out/"
cp -R store/Assets "$out/"
# The Store wants four parts, the last one 0.
sed "s/\$VERSION/$version.0/" store/AppxManifest.xml > "$out/AppxManifest.xml"
echo "$out"
