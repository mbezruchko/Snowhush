#!/bin/zsh
# Full simulation and offscreen Metal checks. Does not launch the app or inspect desktop windows.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
sources=(Sources/Snowhush/*.swift)
sources=(${sources:#*/main.swift})
sources=(${sources:#*/AppDelegate.swift})
swiftc -O -module-cache-path "${TMPDIR:-/tmp}/snowhush-test-modules" \
    "${sources[@]}" Tests/SnowSceneChecks/main.swift -o "$scratch/check"
"$scratch/check" "${1:-/tmp/snowhush-snow-preview.png}"
