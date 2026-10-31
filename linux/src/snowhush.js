// Tells Snowhush where the windows are and where the pointer is, which Wayland keeps from apps.
// Snowhush loads this script into KWin when it starts and unloads it when it quits; it puts
// APP_PID and SHOW_IN_SCREENSHOTS in front of it.

const SERVICE = "io.github.mbezruchko.Snowhush";
const PATH = "/io/github/mbezruchko/Snowhush";
const INTERFACE = "io.github.mbezruchko.Snowhush.KWin";

// Calls in a row that went unanswered, across both senders.
let unanswered = 0;
let probeAt = 0;

// One call in flight at a time: what changes meanwhile goes out, latest only, when the answer comes.
// A call that fails never answers, so after a second the next change goes out anyway. After a few
// unanswered in a row the app is taken to be gone, as after a crash, which leaves this script loaded:
// it then tries only every half a minute, so a dead app doesn't fill the journal with failed calls,
// and picks up again if the app was only stalled.
function sender(method, payload) {
    let busy = false;
    let dirty = false;
    let sentAt = 0;
    function send() {
        const now = Date.now();
        if (busy && now - sentAt < 1000) {
            dirty = true;
            return;
        }
        if (busy) {
            unanswered++;
        }
        if (unanswered >= 5) {
            if (now < probeAt) {
                return;
            }
            probeAt = now + 30000;
        }
        busy = true;
        dirty = false;
        sentAt = now;
        callDBus(SERVICE, PATH, INTERFACE, method, payload(), function () {
            busy = false;
            unanswered = 0;
            if (dirty) {
                send();
            }
        });
    }
    return send;
}

function own(window) {
    return window.pid === APP_PID;
}

// Windows that are there but never seen, such as the bridge that lets X11 apps share a Wayland screen, neither
// hold snow nor hide it.
function invisible(window) {
    return window.opacity < 0.05 || window.resourceClass === "xwaylandvideobridge";
}

function onCurrentDesktop(window) {
    const desktop = workspace.currentDesktop;
    const activity = workspace.currentActivity;
    const onDesktop = window.onAllDesktops || window.desktops.some(function (d) { return d.id === desktop.id; });
    const onActivity = window.activities.length === 0 || window.activities.indexOf(activity) >= 0;
    return onDesktop && onActivity;
}

// Strings rather than numbers: KWin may hand a whole number to D-Bus as an int, which the app wouldn't take.
function cursor() {
    const point = workspace.cursorPos;
    return point.x + " " + point.y;
}

// The displays with their work areas, and the windows front to back.
function windows() {
    const screens = workspace.screens.map(function (screen) {
        const g = screen.geometry;
        const area = workspace.clientArea(KWin.MaximizeArea, screen, workspace.currentDesktop);
        return { name: screen.name, x: g.x, y: g.y, w: g.width, h: g.height, ax: area.x, ay: area.y, aw: area.width, ah: area.height };
    });
    const list = [];
    const elsewhere = [];
    const panels = [];
    let moving = false;
    const order = workspace.stackingOrder;
    for (let i = order.length - 1; i >= 0; i--) {
        const window = order[i];
        if (window.dock && !window.deleted && !window.hidden) {
            const g = window.frameGeometry;
            panels.push({ id: String(window.internalId), x: g.x, y: g.y, w: g.width, h: g.height });
        }
        if (window.deleted || window.desktopWindow || window.dock || own(window) || invisible(window)) {
            continue;
        }
        if (window.move || window.resize) {
            moving = true;
        }
        if (window.minimized || window.hidden) {
            continue;
        }
        const id = String(window.internalId);
        if (!onCurrentDesktop(window)) {
            elsewhere.push(id);
            continue;
        }
        const g = window.frameGeometry;
        list.push({
            id: id,
            class: window.resourceClass,
            x: g.x,
            y: g.y,
            w: g.width,
            h: g.height,
            solid: (window.normalWindow || window.dialog || window.utility) && !window.popupWindow,
            full: window.fullScreen,
            above: window.keepAbove || window.layer > 2,
            decorated: !window.noBorder,
            maximized: window.maximizeMode === 3,
        });
    }
    const desktop = workspace.currentDesktop.id + " " + workspace.currentActivity;
    const columns = Math.max(workspace.desktopGridWidth, 1);
    const index = Math.max(workspace.desktops.findIndex(function (d) { return d.id === workspace.currentDesktop.id; }), 0);
    return JSON.stringify({
        screens: screens,
        windows: list,
        elsewhere: elsewhere,
        panels: panels,
        moving: moving,
        desktop: desktop,
        column: index % columns,
        columns: columns,
        wraps: options.rollOverDesktops,
    });
}

const sendCursor = sender("Cursor", cursor);
const sendWindows = sender("Windows", windows);

function watch(window) {
    if (own(window)) {
        window.excludeFromCapture = !SHOW_IN_SCREENSHOTS;
        return;
    }
    // Stacking changes come per window: a window clicked to the front reports it after it is activated,
    // so the activation alone would leave the order from before the raise.
    const signals = [
        window.stackingOrderChanged,
        window.maximizedChanged,
        window.frameGeometryChanged,
        window.minimizedChanged,
        window.fullScreenChanged,
        window.desktopsChanged,
        window.activitiesChanged,
        window.hiddenChanged,
        window.keepAboveChanged,
        window.interactiveMoveResizeStarted,
        window.interactiveMoveResizeFinished,
    ];
    for (const signal of signals) {
        if (signal) {
            signal.connect(sendWindows);
        }
    }
}

// Not every KWin has every signal; the ones missing are left out, and said so in the journal.
function listen(name, handler) {
    const signal = workspace[name];
    if (signal) {
        signal.connect(handler);
    } else {
        console.log("snowhush: KWin has no " + name);
    }
}

workspace.windowList().forEach(watch);
listen("windowAdded", function (window) {
    watch(window);
    sendWindows();
});
listen("windowRemoved", sendWindows);
listen("windowActivated", sendWindows);
listen("stackingOrderChanged", sendWindows);
listen("currentDesktopChanged", sendWindows);
listen("currentActivityChanged", sendWindows);
listen("screensChanged", sendWindows);
listen("virtualScreenGeometryChanged", sendWindows);
listen("cursorPosChanged", sendCursor);
sendWindows();
sendCursor();
