#!/usr/bin/env bash
# Install for the current user; no root access or system changes are needed.
# A prebuilt binary can be supplied as the first argument.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
binary=${1:-$root/linux/target/release/snowhush}
if [[ ! -x "$binary" ]]; then
    if [[ $# -gt 0 ]]; then
        echo "Not an executable: $binary" >&2
        exit 1
    fi
    cargo build --locked --release --manifest-path "$root/linux/Cargo.toml"
fi
destination=${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/snowhush@mbezruchko.github.io
install -d "$destination/bin"
install -m644 "$here/metadata.json" "$here/extension.js" "$destination/"
install -m755 "$binary" "$destination/bin/snowhush"
install -m644 "$root/LICENSE" "$destination/LICENSE"
install -Dm644 "$root/linux/packaging/snowhush.desktop" "${XDG_DATA_HOME:-$HOME/.local/share}/applications/snowhush.desktop"
install -Dm644 "$root/windows/assets/icon-256.png" "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps/snowhush.png"
# A desktop entry must work even when ~/.local/bin is absent from the session's PATH.
python3 - "$destination/bin/snowhush" "${XDG_DATA_HOME:-$HOME/.local/share}/applications/snowhush.desktop" <<'PY'
import pathlib, sys
binary, desktop = sys.argv[1:]
escaped = binary.replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
p = pathlib.Path(desktop)
p.write_text(p.read_text().replace('Exec=snowhush', f'Exec="{escaped}"'))
PY
echo "Installed: $destination"
echo 'Enable with: gnome-extensions enable snowhush@mbezruchko.github.io'
echo 'If GNOME does not see the extension yet, log out and back in first.'
