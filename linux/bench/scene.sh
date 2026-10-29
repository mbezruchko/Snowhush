#!/usr/bin/env bash
# One scene of the bench, inside its container: see run.sh.
set -u
export XDG_RUNTIME_DIR=/tmp/runtime
mkdir -p -m 700 "$XDG_RUNTIME_DIR"
export QT_FORCE_STDERR_LOGGING=1 LANG=C.UTF-8
app=/target/release/snowhush
log=~/.local/state/snowhush/snowhush.log

trace() { echo "$1" >> /out/trace; }

# Waits for a new Wayland socket other than `$1`, and prints its name.
socket() {
    for _ in $(seq 100); do
        local found
        found=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | grep -vx "${1:-}" | head -1)
        [[ -n $found ]] && { echo "$found"; return; }
        sleep 0.2
    done
}

# Sway without a display, which the other compositors can run inside, and grim can photograph.
headless_sway() {
    export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
    mkdir -p ~/.config/sway
    cat > ~/.config/sway/config <<'CONFIG'
output HEADLESS-1 resolution 1280x800 position 0 0 bg #1d2b44 solid_color
default_border pixel 2
for_window [app_id="niri"] border none, fullscreen enable
for_window [app_id="foot"] floating enable, move position 200 300
CONFIG
    sway > /out/sway.log 2>&1 &
    sway_display=$(socket)
}

# Snowfall and the drift on the ground, photographed twice after the snow has had time to build.
snowfall() {
    foot -o tweak.font-monospace-warn=no -W 70x16 -e sleep 600 2>/dev/null &
    sleep 3
    WAYLAND_DISPLAY=$sway_display grim /out/before.png
    "$app" > /out/app.out 2>&1 &
    sleep 25
    WAYLAND_DISPLAY=$sway_display grim /out/snow-1.png
    sleep 10
    WAYLAND_DISPLAY=$sway_display grim /out/snow-2.png
    trace "$(grep -E 'adapter|display|frames' "$log")"
}

case $1 in
sway)
    headless_sway
    export WAYLAND_DISPLAY=$sway_display
    snowfall
    ;;
niri)
    headless_sway
    mkdir -p ~/.config/niri
    printf 'prefer-no-csd\nhotkey-overlay { skip-at-startup; }\nwindow-rule { open-floating true; }\n' > ~/.config/niri/config.kdl
    WAYLAND_DISPLAY=$sway_display niri > /out/niri.log 2>&1 &
    export WAYLAND_DISPLAY=$(socket "$sway_display")
    snowfall
    ;;
kwin)
    # Screenshots aren't to be had from the virtual backend, so the scene reads the logs instead: KWin's
    # counts the calls its script makes to an app that is gone.
    cat > /tmp/session.sh <<SESSION
#!/usr/bin/env bash
foot -W 70x16 -e sleep 600 2>/dev/null &
"$app" > /out/app.out 2>&1 &
sleep 8
echo "reports reaching the app: \$(grep -c 'windows front' $log)" >> /out/trace
kill -9 \$!
for _ in \$(seq 40); do foot -e sleep 0.7 2>/dev/null & sleep 1.5; done
echo "calls failing in the minute after a kill: \$(grep -c 'Received D-Bus message is error' /out/kwin.log)" >> /out/trace
SESSION
    chmod +x /tmp/session.sh
    QT_LOGGING_RULES="kwin_scripting.debug=true" kwin_wayland --virtual --width 1280 --height 800 --no-lockscreen \
        --exit-with-session /tmp/session.sh > /out/kwin.log 2>&1
    ;;
*)
    echo "no scene $1" >&2
    exit 1
    ;;
esac
cp "$log" /out/app.log 2>/dev/null
