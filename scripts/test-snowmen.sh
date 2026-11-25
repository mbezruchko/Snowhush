#!/bin/zsh
# Exercise native canvas interactions without modifying saved desktop figures.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
swiftc -module-cache-path "${TMPDIR:-/tmp}/snowhush-test-modules" \
    Sources/Snowhush/SnowWeather.swift Sources/Snowhush/SnowmanMotion.swift Sources/Snowhush/SnowLayer.swift Sources/Snowhush/SnowSculpting.swift \
    Sources/Snowhush/SnowmanCanvas.swift Tests/SnowmanCanvasChecks/main.swift \
    -o "$scratch/check"
"$scratch/check"
