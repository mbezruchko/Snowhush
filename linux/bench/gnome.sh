#!/usr/bin/env bash
# Real GNOME/Mutter integration in isolated XDG directories and a separate session bus.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
binary=${1:-$root/linux/target/debug/snowhush}
out=${2:-$(mktemp -d -t snowhush-gnome.XXXXXX)}
mkdir -p "$out"
out=$(realpath "$out")
if [[ ${SNOWHUSH_BENCH_CHILD:-0} != 1 ]]; then
    exec dbus-run-session -- env SNOWHUSH_BENCH_CHILD=1 "$0" "$binary" "$out"
fi
export SNOWHUSH_BENCH_DIR=$out
export XDG_DATA_HOME=$out/share XDG_CONFIG_HOME=$out/config XDG_CACHE_HOME=$out/cache XDG_STATE_HOME=$out/state
export XDG_RUNTIME_DIR=$out/runtime XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=wayland
export GSETTINGS_BACKEND=keyfile LIBGL_ALWAYS_SOFTWARE=1 GTK_A11Y=none NO_AT_BRIDGE=1
unset WAYLAND_SOCKET WAYLAND_DISPLAY DISPLAY
mkdir -m700 -p "$XDG_RUNTIME_DIR"
destination=$XDG_DATA_HOME/gnome-shell/extensions
mkdir -p "$destination/snowhush@mbezruchko.github.io/bin" "$destination/snowhush-bench@local"
cp "$root"/linux/gnome/{extension.js,metadata.json} "$destination/snowhush@mbezruchko.github.io/"
cp "$binary" "$destination/snowhush@mbezruchko.github.io/bin/snowhush"
cp "$here"/gnome-observer/{extension.js,metadata.json} "$destination/snowhush-bench@local/"
gsettings set org.gnome.shell enabled-extensions "['snowhush@mbezruchko.github.io', 'snowhush-bench@local']"
gsettings set org.gnome.shell disabled-extensions "['ding@rastersoft.com', 'ubuntu-dock@ubuntu.com', 'ubuntu-appindicators@ubuntu.com']"
gsettings set org.gnome.shell welcome-dialog-last-shown-version '46.0'
gsettings set org.gnome.desktop.interface enable-animations false
gnome-shell --headless --virtual-monitor 1280x720 --wayland-display snowhush-bench --no-x11 --sm-disable > "$out/shell.log" 2>&1 &
shell_pid=$!
window_pid=
trap 'kill "$shell_pid" ${window_pid:+"$window_pid"} 2>/dev/null || true' EXIT
for ((i=0; i<300; i++)); do
    if [[ -S "$XDG_RUNTIME_DIR/snowhush-bench" ]]; then break; fi
    sleep 0.1
done
WAYLAND_DEBUG=${SNOWHUSH_BENCH_WAYLAND_DEBUG:-0} WAYLAND_DISPLAY=snowhush-bench gjs -m "$here/gnome-window.js" > "$out/window.log" 2>&1 &
window_pid=$!
for ((i=0; i<900; i++)); do
    if [[ -f "$out/result.json" ]]; then
        cat "$out/result.json"
        echo
        echo "GNOME bench artifacts: $out"
        python3 -c 'import json,sys; sys.exit(json.load(open(sys.argv[1]))["error"] is not None)' "$out/result.json"
        exit
    fi
    if ! kill -0 "$shell_pid" 2>/dev/null; then break; fi
    sleep 0.1
done
echo "GNOME bench failed or timed out; see $out/shell.log" >&2
exit 1
