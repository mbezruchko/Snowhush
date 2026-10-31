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

# Sway without a display, which the other compositors can run inside, and grim can photograph. `$second`, if set,
# is where a second display goes.
headless_sway() {
    export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
    [[ -n ${second:-} ]] && export WLR_HEADLESS_OUTPUTS=2
    mkdir -p ~/.config/sway
    cat > ~/.config/sway/config <<CONFIG
output HEADLESS-1 resolution 1280x800 position 0 0 bg #1d2b44 solid_color
${second:+output HEADLESS-2 resolution 1280x800 position $second bg #24344f solid_color}
default_border pixel 2
for_window [app_id="niri"] border none, fullscreen enable
for_window [app_id="foot"] floating enable, move position 200 300
CONFIG
    sway > /out/sway.log 2>&1 &
    sway_display=$(socket)
}

# Snowfall on a floating window and on the ground, photographed as the window's snow is put through what a
# source has to get right: the cap once it has built up, carried along when the window moves slowly, kept
# waiting while the window is on another workspace and back on it after, and falling when it closes.
# `$1` names the compositor, whose `_move`, `_workspace` and `_close` do those things to the window.
snowfall() {
    shot() { WAYLAND_DISPLAY=$sway_display grim "/out/$1.png"; }
    foot -o tweak.font-monospace-warn=no -W 70x16 -e sleep 600 2>/dev/null &
    sleep 3
    shot before
    "$app" > /out/app.out 2>&1 &
    sleep 25
    shot snow
    for _ in $(seq 30); do "$1_move" 4; sleep 0.05; done
    sleep 1
    shot carried
    "$1_workspace" 2
    sleep 2
    shot away
    "$1_workspace" 1
    sleep 1
    shot back
    # Where the compositor has an overview, the snow hides while it is open.
    if [[ $(type -t "$1_overview") ]]; then
        "$1_overview"
        sleep 1
        shot overview
        "$1_overview"
        sleep 1
    fi
    "$1_close"
    sleep 1.5
    shot closed
    trace "$(grep -E 'adapter|display|windows from|frames' "$log")"
}

# Two displays, side by side or one under the other as `$1` says: a window's snow left lying across the edge between
# them, shaken off over the other display, and carried over to it as the window is dragged across.
displays() {
    shot() { grim "/out/$1.png"; }
    drag() { for _ in $(seq "$2"); do swaymsg "[app_id=foot] move $1 8 px" > /dev/null; sleep 0.05; done; }
    foot -o tweak.font-monospace-warn=no -W 70x16 -e sleep 600 2>/dev/null &
    sleep 3
    if [[ $1 == side ]]; then
        swaymsg "[app_id=foot] move absolute position 200 300" > /dev/null
    else
        swaymsg "[app_id=foot] move absolute position 300 160" > /dev/null
    fi
    "$app" > /out/app.out 2>&1 &
    sleep 25
    shot 1-snow
    if [[ $1 == side ]]; then
        drag right 100
    else
        drag down 70
    fi
    sleep 1
    shot 2-across
    sleep 30
    shot 3-settled
    local way=left back=right
    [[ $1 == stacked ]] && way=up back=down
    for _ in $(seq 8); do
        swaymsg "[app_id=foot] move $way 60 px" > /dev/null
        sleep 0.04
        swaymsg "[app_id=foot] move $back 60 px" > /dev/null
        sleep 0.04
    done
    sleep 3
    shot 4-shaken
    sleep 15
    if [[ $1 == side ]]; then
        drag right 110
    else
        drag down 30
    fi
    sleep 1
    shot 5-over
    sleep 15
    shot 6-later
    trace "$(grep -E 'display|windows from|window snow from' "$log")"
}

sway_move() { swaymsg "[app_id=foot] move right $1 px" > /dev/null; }
sway_workspace() { swaymsg workspace "$1" > /dev/null; }
sway_close() { swaymsg "[app_id=foot] kill" > /dev/null; }
niri_move() { niri msg action move-floating-window -x "+$1" > /dev/null; }
niri_workspace() { niri msg action focus-workspace "$1" > /dev/null; }
niri_close() { niri msg action close-window > /dev/null; }
niri_overview() { niri msg action toggle-overview > /dev/null; }

case $1 in
sway)
    headless_sway
    export WAYLAND_DISPLAY=$sway_display
    SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*)
    export SWAYSOCK
    snowfall sway
    ;;
niri)
    headless_sway
    mkdir -p ~/.config/niri
    printf 'prefer-no-csd\nhotkey-overlay { skip-at-startup; }\nwindow-rule { open-floating true; }\n' > ~/.config/niri/config.kdl
    WAYLAND_DISPLAY=$sway_display niri > /out/niri.log 2>&1 &
    export WAYLAND_DISPLAY=$(socket "$sway_display")
    sleep 1
    NIRI_SOCKET=$(ls "$XDG_RUNTIME_DIR"/niri.*.sock)
    export NIRI_SOCKET
    snowfall niri
    ;;
side | stacked)
    second=$([[ $1 == side ]] && echo "1280 0" || echo "0 800")
    headless_sway
    export WAYLAND_DISPLAY=$sway_display
    SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*)
    export SWAYSOCK
    displays "$1"
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
