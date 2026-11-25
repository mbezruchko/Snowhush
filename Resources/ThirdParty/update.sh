#!/bin/zsh
# Writes the third-party notices the apps ship: Notices.txt for Windows and Linux, the depth network's own notice
# (Depth.txt) and the licenses of every Rust crate they link; Mac-Notices.txt for the Mac app's Rust crates, the rooms
# of snowball fights. Run after dependencies change. Needs `cargo install cargo-about --features cli`.
set -euo pipefail
here=${0:A:h}
root=${here:h:h}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

report() {
    print "accepted = $(sed -n '/^accepted/,/^]/p' "$here/about.toml" | sed '1s/^accepted = //' | tr -d '\n')" > "$work/$1.toml"
    print "targets = [$2]" >> "$work/$1.toml"
    cargo about generate --format json -c "$work/$1.toml" -m "$root/$1/Cargo.toml" -o "$work/$1.json"
}
report windows '"x86_64-pc-windows-msvc"'
report linux '"x86_64-unknown-linux-gnu"'
report net '"aarch64-apple-darwin", "x86_64-apple-darwin"'

python3 "$here/render.py" "$work/crates.txt" "Rust crates" "$work/windows.json" "$work/linux.json"
{ cat "$here/Depth.txt"; print; cat "$work/crates.txt"; } > "$here/Notices.txt"
python3 "$here/render.py" "$here/Mac-Notices.txt" "Snowhush for the Mac carries the work of others, under the licenses below: the Rust crates of its snowball fights' rooms." "$work/net.json"
