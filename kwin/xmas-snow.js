// xmas-tui window-snow bridge for KWin (Plasma 6).
// Wayland doesn't let ordinary apps see where other windows are, so this
// tiny KWin script reports the visible windows (geometry, kind, stacking),
// the ids of windows that are only on another desktop / activity (so their
// snow is kept for them) and the cursor position to xmas-tui over D-Bus.
// Loaded and unloaded by xmas-tui itself; it does nothing else.

const SVC = "org.xmastui.Snow";
const PATH = "/Snow";
const IFACE = "org.xmastui.Snow";
let last = 0;
let lastCursor = 0;
// the window list as JSON, rebuilt only when something about the windows
// changed (cursor-only reports reuse it), and at least once a second as a
// safety net for changes no signal told us about
let list = "[]";
let away = "[]";
let dirty = true;
let built = 0;
// a throttled window change still owed to xmas-tui: sent by `trail` once
// the throttle interval is over, so the last geometry always arrives
let pending = false;
const trail = new QTimer();
trail.singleShot = true;
trail.timeout.connect(function () {
    if (pending) send();
});

function onThisDesktop(w) {
    return (!w.desktops.length || w.desktops.indexOf(workspace.currentDesktop) !== -1) &&
           (!w.activities.length || w.activities.indexOf(workspace.currentActivity) !== -1);
}

// a window that would get snow if it were on this desktop / activity
function eligible(w) {
    if (w.deleted || w.minimized) return false;
    if (w.resourceClass === "xmas-tui" || w.resourceClass === "xmas-snow" || w.resourceClass === "kwin_wayland") return false;
    // KWin layers: 0 desktop, 1 below, 2 normal, 3 above, 4 notification,
    // 5 active, 6+ popups/OSD/overlays (incl. layer-shell surfaces like ours)
    if (w.layer === 0 || w.layer === 4 || w.layer >= 6) return false;
    return w.normalWindow || w.dialog || w.dock;
}

function kind(w) {
    if (w.fullScreen) return 2;
    if (w.dock) return 1;
    const area = workspace.clientArea(KWin.MaximizeArea, w);
    const g = w.frameGeometry;
    if (g.width >= area.width && g.height >= area.height) return 3; // maximized
    return 0;
}

function send() {
    last = Date.now();
    pending = false;
    if (dirty || last - built > 1000) {
        dirty = false;
        built = last;
        const all = workspace.windowList().filter(eligible);
        const here = [], there = [];
        all.forEach(function (w) {
            (onThisDesktop(w) ? here : there).push(w);
        });
        list = JSON.stringify(here.map(function (w) {
            const g = w.frameGeometry;
            return [w.internalId.toString(), Math.round(g.x), Math.round(g.y), Math.round(g.width), Math.round(g.height), kind(w), w.stackingOrder];
        }));
        away = JSON.stringify(there.map(function (w) { return w.internalId.toString(); }));
    }
    const c = workspace.cursorPos;
    callDBus(SVC, PATH, IFACE, "Update", '{"w":' + list + ',"a":' + away + ',"c":[' + c.x + ',' + c.y + ']}');
}

function changed() {
    dirty = true;
    send();
}

function soon() {
    // window moves: up to ~240 updates/s so snow and lights keep up, plus
    // a trailing send for a change that came in too soon after the last one
    dirty = true;
    const wait = 4 - (Date.now() - last);
    if (wait <= 0) {
        send();
    } else {
        pending = true;
        if (!trail.active) {
            trail.interval = wait;
            trail.start();
        }
    }
}

function cursorMoved() {
    // the cursor only matters for brushing snow, and the overlay looks at
    // it once a frame (30/s) unless windows are moving
    const now = Date.now();
    if (now - lastCursor >= 30 && now - last >= 4) {
        lastCursor = now;
        send();
    }
}

function hook(w) {
    w.frameGeometryChanged.connect(soon);
    w.minimizedChanged.connect(changed);
    w.fullScreenChanged.connect(changed);
    w.interactiveMoveResizeFinished.connect(changed);
    w.stackingOrderChanged && w.stackingOrderChanged.connect(soon);
    w.desktopsChanged && w.desktopsChanged.connect(changed);
    w.activitiesChanged && w.activitiesChanged.connect(changed);
}

workspace.windowList().forEach(hook);
workspace.windowAdded.connect(function (w) { hook(w); changed(); });
workspace.windowRemoved.connect(changed);
workspace.windowActivated.connect(changed);
workspace.currentDesktopChanged.connect(changed);
workspace.currentActivityChanged && workspace.currentActivityChanged.connect(changed);
workspace.cursorPosChanged.connect(cursorMoved);
send();
