#!/bin/zsh
# Builds build/Snowhush.app for Apple silicon and Intel.
# Signs ad hoc for local use; set IDENTITY to a "Developer ID Application" identity for a release build.
set -euo pipefail
cd "$(dirname "$0")"

flags=(-c release --arch arm64 --arch x86_64)
swift build "${flags[@]}"
products="$(swift build "${flags[@]}" --show-bin-path)"

app=build/Snowhush.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
# Swift Build stamps the binary with the deployment target as its SDK, and macOS then draws the app in its old
# look, without Liquid Glass.
minimum=$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' Resources/Info.plist)
vtool -set-build-version macos "$minimum" "$(xcrun --sdk macosx --show-sdk-version)" -replace \
    -output "$app/Contents/MacOS/Snowhush" "$products/Snowhush"
ditto "$products/Sparkle.framework" "$app/Contents/Frameworks/Sparkle.framework"
cp Resources/Info.plist "$app/Contents/"
cp Resources/AppIcon.icns "$app/Contents/Resources/"

# The depth network that finds the tops of things in the wallpaper for snow to lie on: Apple's Core ML conversion of
# Depth Anything V2 Small, palettized to 6 bits (Apache-2.0). Fetched once at a fixed revision, checked, compiled.
model=DepthAnythingV2SmallF16P6
models=build/models
if [[ ! -d "$models/$model.mlmodelc" ]]; then
    revision=cfef6f6f2a70783dedc0bfae40cecbc2052285d3
    base="https://huggingface.co/apple/coreml-depth-anything-v2-small/resolve/$revision/$model.mlpackage"
    package="$models/$model.mlpackage"
    while read -r sum file; do
        mkdir -p "$package/${file:h}"
        curl -sfL -o "$package/$file" "$base/$file"
        echo "$sum  $package/$file" | shasum -a 256 -c --quiet
    done <<'FILES'
70e469c0359c89851bde4cdc1d89bf13fa06d4fccd8907b9b5f744e548ecaadf Manifest.json
ee53b8f7e1682a66857b5a3d54cf04a97d89a98cf7b5979b5d726aaf9c24ad70 Data/com.apple.CoreML/model.mlmodel
3e9bcccc9e35ae6f9759551f481a3b77226510bd74536e059ee9780dda618381 Data/com.apple.CoreML/weights/weight.bin
FILES
    xcrun coremlcompiler compile "$package" "$models" >/dev/null
fi
ditto "$models/$model.mlmodelc" "$app/Contents/Resources/$model.mlmodelc"
cp Resources/ThirdParty/DepthAnything-LICENSE.txt "$app/Contents/Resources/"
for strings in locales/*.strings; do
    lproj="$app/Contents/Resources/$(basename "$strings" .strings).lproj"
    mkdir -p "$lproj"
    plutil -convert binary1 -o "$lproj/Localizable.strings" "$strings"
done

if [[ -n "${IDENTITY:-}" ]]; then
    # Sparkle's helpers are signed inside out, each with the hardened runtime, as notarization requires.
    sparkle="$app/Contents/Frameworks/Sparkle.framework/Versions/B"
    sign=(codesign --force --options runtime --timestamp --sign "$IDENTITY")
    "${sign[@]}" "$sparkle/XPCServices/Installer.xpc"
    "${sign[@]}" --preserve-metadata=entitlements "$sparkle/XPCServices/Downloader.xpc"
    "${sign[@]}" "$sparkle/Autoupdate"
    "${sign[@]}" "$sparkle/Updater.app"
    "${sign[@]}" "$app/Contents/Frameworks/Sparkle.framework"
    "${sign[@]}" "$app"
else
    codesign --force --deep --sign - "$app"
fi
codesign --verify --deep --strict "$app"
echo "Built $app"
