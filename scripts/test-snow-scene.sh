#!/bin/zsh
# Full simulation and offscreen Metal checks. Does not launch the app or inspect desktop windows.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ! -f build/net/libsnowhush_net.a ]]; then
    print -u2 'Missing build/net/libsnowhush_net.a. Build the Rust network library with build.sh first (cargo required).'
    exit 1
fi
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
sources=(Sources/Snowhush/*.swift)
sources=(${sources:#*/main.swift})
sources=(${sources:#*/AppDelegate.swift})
swiftc -O -module-cache-path "${TMPDIR:-/tmp}/snowhush-test-modules" \
    -I Sources/CSnowhushNet -L build/net -framework SystemConfiguration -framework Security \
    "${sources[@]}" Tests/SnowSceneChecks/main.swift -o "$scratch/check"
"$scratch/check" "${1:-/tmp/snowhush-snow-preview.png}"
