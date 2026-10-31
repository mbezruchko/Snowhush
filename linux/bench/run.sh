#!/usr/bin/env bash
# Runs the Linux app in a compositor inside a container, without a display, and leaves what happened in
# linux/bench/out/<scene>: a trace, the app's and the compositor's logs, and for Sway and niri, screenshots.
# Drawing is in software, so it shows whether things work, not how fast. Needs Docker.
#
#   linux/bench/run.sh sway    snowfall and the drift on the ground
#   linux/bench/run.sh niri    the same, in niri
#   linux/bench/run.sh kwin    the KWin script reports windows, and goes quiet once the app is killed
#   linux/bench/run.sh side    window snow across two displays side by side, in Sway
#   linux/bench/run.sh stacked the same with one display under the other
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
scene=${1:?usage: run.sh sway|niri|kwin|side|stacked}
out=$here/out/$scene
rm -rf "$out"
mkdir -p "$out"
chmod 777 "$out"

docker build -q -t snowhush-bench "$here" >/dev/null
echo "Building snowhush…"
docker run --rm -v "$root:/src:ro" -v snowhush-bench-target:/target -v snowhush-bench-cargo:/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target -w /src/linux rust:1-bookworm \
    bash -c "apt-get update -qq && apt-get install -y -qq libwayland-dev >/dev/null && cargo build --release --locked -q"
echo "Running ${scene}…"
docker run --rm -v "$here:/bench:ro" -v "$out:/out" -v snowhush-bench-target:/target:ro --user tester snowhush-bench \
    dbus-run-session /bench/scene.sh "$scene"
cat "$out/trace"
