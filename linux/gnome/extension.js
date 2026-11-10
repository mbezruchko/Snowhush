import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const SERVICE = 'io.github.mbezruchko.Snowhush';
const PATH = '/io/github/mbezruchko/Snowhush';
const IFACE = `${SERVICE}.Gnome`;
const SHELL_PATH = `${PATH}/Gnome`;
const SHELL_XML = `<node><interface name="${SERVICE}.Shell"><method name="Start"/></interface></node>`;
const SOLID = new Set([Meta.WindowType.NORMAL, Meta.WindowType.DIALOG,
    Meta.WindowType.MODAL_DIALOG, Meta.WindowType.UTILITY, Meta.WindowType.SPLASHSCREEN]);
const COVER = new Set([Meta.WindowType.MENU, Meta.WindowType.DROPDOWN_MENU,
    Meta.WindowType.POPUP_MENU, Meta.WindowType.TOOLTIP, Meta.WindowType.NOTIFICATION,
    Meta.WindowType.COMBO]);

// This extension only integrates with Mutter. Simulation, assets and rendering stay in Rust.
export default class SnowhushExtension extends Extension {
    enable() {
        if (!Meta.is_wayland_compositor()) {
            Main.notify('Snowhush', 'Snowhush requires a GNOME Wayland session.');
            return;
        }
        this._active = true;
        this._windows = new Map();
        this._overlays = new Map();
        this._actors = new Map();
        this._painted = new Set();
        this._settings = {};
        this._sending = false;
        this._moving = false;
        this._buildMenu();
        this._export = Gio.DBusExportedObject.wrapJSObject(SHELL_XML, {Start: () => this._start()});
        this._export.export(Gio.DBus.session, SHELL_PATH);
        this._signals = [
            [global.display, global.display.connect('window-created', (_display, window) => this._watchWindow(window))],
            [global.display, global.display.connect('grab-op-begin', () => { this._moving = true; })],
            [global.display, global.display.connect('grab-op-end', () => { this._moving = false; })],
            [Main.overview, Main.overview.connect('showing', () => this._visibility())],
            [Main.overview, Main.overview.connect('hidden', () => this._visibility())],
            [Main.sessionMode, Main.sessionMode.connect('updated', () => this._visibility())],
        ];
        this._nameWatch = Gio.bus_watch_name(Gio.BusType.SESSION, SERVICE,
            Gio.BusNameWatcherFlags.NONE,
            () => {
                this._ready = true;
                this._lastReport = null;
                this._call('Settings', null, result => {
                    if (!this._active)
                        return;
                    this._settings = JSON.parse(result.deepUnpack()[0]);
                    this._updateMenu();
                });
            },
            () => { this._ready = false; });
        this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 33, () => {
            this._sample();
            return GLib.SOURCE_CONTINUE;
        });
        this._start();
    }

    disable() {
        this._active = false;
        this._export?.unexport();
        this._export = null;
        if (this._timer)
            GLib.source_remove(this._timer);
        this._timer = 0;
        if (this._nameWatch)
            Gio.bus_unwatch_name(this._nameWatch);
        this._nameWatch = 0;
        for (const [object, id] of this._signals ?? [])
            object.disconnect(id);
        this._signals = [];
        this._stop();
        for (const window of [...(this._windows?.keys() ?? [])])
            this._forgetWindow(window);
        this._indicator?.destroy();
        this._indicator = null;
        this._ready = false;
    }

    _call(method, parameters, callback = null) {
        Gio.DBus.session.call(SERVICE, PATH, IFACE, method, parameters, null,
            Gio.DBusCallFlags.NO_AUTO_START, 1000, null, (connection, result) => {
                try {
                    const reply = connection.call_finish(result);
                    callback?.(reply);
                } catch (error) {
                    // A process stopping is ordinary; never retry in an unbounded queue.
                    if (method === 'Report')
                        this._lastReport = null;
                    if (method !== 'Report' && this._active)
                        console.error(`Snowhush ${method}: ${error.message}`);
                } finally {
                    if (method === 'Report')
                        this._sending = false;
                }
            });
    }

    _start() {
        if (this._process)
            return;
        const binary = `${this.path}/bin/snowhush`;
        if (!GLib.file_test(binary, GLib.FileTest.IS_EXECUTABLE))
            throw new Error('Build/install Snowhush using linux/gnome/install.sh first.');
        const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
        const argv = [binary, '--gnome-engine'];
        // Trusted client ownership prevents another application spoofing an overlay's app ID/title.
        if (Meta.WaylandClient.new_subprocess) {
            this._client = Meta.WaylandClient.new_subprocess(global.context, launcher, argv);
            this._process = this._client.get_subprocess();
        } else {
            this._client = Meta.WaylandClient.new(global.context, launcher);
            this._process = this._client.spawnv(global.display, argv);
        }
        const process = this._process;
        process.wait_async(null, (_process, result) => {
            process.wait_finish(result);
            if (this._process !== process)
                return;
            this._process = null;
            this._client = null;
            this._ready = false;
            if (this._active) {
                this._startItem.label.text = 'Start Snowhush';
                if (!process.get_successful())
                    Main.notify('Snowhush', 'The Rust engine stopped. See ~/.local/state/snowhush/snowhush.log.');
            }
        });
        this._startItem.label.text = 'Stop Snowhush';
    }

    _stop() {
        const process = this._process;
        if (!process)
            return;
        // SIGTERM is handled by the Rust event loop, which releases EGL and Wayland resources.
        process.send_signal(15);
        if (this._startItem)
            this._startItem.label.text = 'Start Snowhush';
    }

    _watchWindow(window) {
        if (!this._client?.owns_window(window) || this._windows.has(window))
            return;
        const ids = [
            window.connect('notify::title', () => this._place(window)),
            window.connect('shown', () => this._place(window)),
            window.connect('unmanaged', () => this._forgetWindow(window)),
        ];
        this._windows.set(window, ids);
        this._place(window);
    }

    _forgetWindow(window) {
        for (const id of this._windows.get(window) ?? [])
            window.disconnect(id);
        const record = this._actors.get(window);
        if (record) {
            record.actor.disconnect(record.id);
            record.actor.disconnect(record.destroyId);
        }
        this._actors.delete(window);
        this._painted.delete(window);
        this._windows.delete(window);
        this._overlays.delete(window);
    }

    _place(window) {
        if (!this._client?.owns_window(window))
            return;
        const title = window.get_title() ?? '';
        if (!title.startsWith('snowhush:'))
            return;
        let data;
        try {
            data = JSON.parse(title.slice('snowhush:'.length));
        } catch {
            return;
        }
        this._overlays.set(window, data);
        const actor = window.get_compositor_private();
        if (actor && !this._actors.has(window) && !this._painted.has(window)) {
            const record = {actor, id: 0, destroyId: 0};
            record.id = actor.connect('first-frame', () => {
                this._painted.add(window);
                actor.disconnect(record.id);
                actor.disconnect(record.destroyId);
                this._actors.delete(window);
            });
            // WindowActor can be disposed before Meta.Window emits unmanaged.
            record.destroyId = actor.connect('destroy', () => {
                this._actors.delete(window);
                this._painted.delete(window);
            });
            this._actors.set(window, record);
        }
        // Mutter 46 exposes these operations on WaylandClient; newer Mutter also has Window.set_type.
        const type = data.behind ? Meta.WindowType.DESKTOP : Meta.WindowType.DOCK;
        if (window.get_window_type() === type) {
            // Already classified; repeated placement must not call make_dock again.
        } else if (window.set_type) {
            window.set_type(type);
            window.hide_from_window_list?.();
        } else {
            if (data.behind)
                this._client.make_desktop(window);
            else
                this._client.make_dock(window);
            this._client.hide_from_window_list(window);
        }
        window.stick();
        const [x, y, width, height] = data.area.map(Math.round);
        const frame = window.get_frame_rect();
        if (frame.x !== x || frame.y !== y || frame.width !== width || frame.height !== height)
            window.move_resize_frame(false, x, y, width, height);
        this._visibility();
    }

    _hidden() {
        return Main.overview.visible || Main.sessionMode.isLocked || Main.sessionMode.isGreeter;
    }

    _visibility() {
        const hidden = this._hidden();
        for (const [window, data] of this._overlays) {
            const actor = window.get_compositor_private();
            if (actor)
                actor.visible = !hidden;
            // A just-created Meta.Window is not in Mutter's stack until its actor maps.
            if (!actor?.mapped || !this._painted.has(window))
                continue;
            if (!hidden && data.over && !data.behind && !window.is_above())
                window.make_above();
            else if ((!data.over || data.behind) && window.is_above())
                window.unmake_above();
        }
    }

    _sample() {
        this._visibility();
        if (!this._ready || this._sending)
            return;
        const workspace = global.workspace_manager.get_active_workspace();
        const report = {screens: [], windows: [], elsewhere: [], moving: this._moving,
            hidden: this._hidden(), desktop: String(workspace.index())};
        report.screens = Main.layoutManager.monitors.map((monitor, index) => {
            const area = Main.layoutManager.getWorkAreaForMonitor(index);
            const data = [...this._overlays.values()].find(d =>
                d.bounds[0] === monitor.x && d.bounds[1] === monitor.y);
            return {name: data?.name ?? `monitor-${index}`,
                x: monitor.x, y: monitor.y, w: monitor.width, h: monitor.height,
                ax: area.x, ay: area.y, aw: area.width, ah: area.height};
        });
        for (const actor of global.get_window_actors().slice().reverse()) {
            const window = actor.meta_window;
            if (!window || this._client?.owns_window(window))
                continue;
            const type = window.get_window_type();
            const solid = SOLID.has(type);
            if (!solid && !COVER.has(type))
                continue;
            const id = String(window.get_stable_sequence());
            if (!window.located_on_workspace(workspace)) {
                if (!window.minimized)
                    report.elsewhere.push(id);
                continue;
            }
            if (window.minimized || !actor.visible || !window.showing_on_its_workspace())
                continue;
            const r = window.get_frame_rect();
            report.windows.push({id, class: window.get_wm_class() ?? '',
                x: r.x, y: r.y, w: r.width, h: r.height, solid,
                full: window.is_fullscreen(), above: window.is_above(), decorated: true,
                maximized: window.is_maximized?.() ?? window.get_maximized() === Meta.MaximizeFlags.BOTH});
        }
        const [x, y] = global.get_pointer();
        const state = JSON.stringify(report);
        const signature = `${x},${y}:${state}`;
        if (signature === this._lastReport)
            return;
        this._lastReport = signature;
        this._sending = true;
        this._call('Report', new GLib.Variant('(sdd)', [state, x, y]));
    }

    _control(key, value) {
        if (!this._ready)
            return;
        this._settings[key] = value;
        this._call('Control', new GLib.Variant('(si)', [key, Number(value)]));
        this._updateMenu();
    }

    _buildMenu() {
        this._indicator = new PanelMenu.Button(0.0, 'Snowhush');
        this._indicator.add_child(new St.Icon({icon_name: 'weather-snow-symbolic', style_class: 'system-status-icon'}));
        this._startItem = new PopupMenu.PopupMenuItem('Start Snowhush');
        this._startItem.connect('activate', () => this._process ? this._stop() : this._start());
        this._indicator.menu.addMenuItem(this._startItem);
        this._toggles = new Map();
        this._submenus = new Map();
        for (const [key, label] of [['ground', 'Accumulate on ground'], ['windows', 'Accumulate on windows'],
            ['melt', 'Melt / wither'], ['behind', 'Behind windows'], ['fullscreen', 'Show over full screen'],
            ['performance', 'Maximum performance'], ['halloween', 'Halloween']]) {
            const item = new PopupMenu.PopupSwitchMenuItem(label, false);
            item.connect('toggled', (_item, value) => { if (!this._updating) this._control(key, value); });
            this._indicator.menu.addMenuItem(item);
            this._toggles.set(key, item);
        }
        for (const [key, label, values] of [['intensity', 'Intensity', [0, 25, 50, 75, 100]],
            ['opacity', 'Opacity', [20, 40, 60, 80, 100]], ['mode', 'Mode', [0, 1, 2, 3]]]) {
            const submenu = new PopupMenu.PopupSubMenuMenuItem(label);
            this._submenus.set(key, {submenu, label});
            for (const value of values) {
                const name = key === 'mode' ? ['Snow', 'Autumn leaves', 'Rain', 'Cherry blossom'][value] : `${value}%`;
                const item = new PopupMenu.PopupMenuItem(name);
                item.connect('activate', () => this._control(key, value));
                submenu.menu.addMenuItem(item);
            }
            this._indicator.menu.addMenuItem(submenu);
        }
        const clear = new PopupMenu.PopupMenuItem('Clear');
        clear.connect('activate', () => this._control('clear', 1));
        this._indicator.menu.addMenuItem(clear);
        Main.panel.addToStatusArea(this.uuid, this._indicator);
    }

    _updateMenu() {
        this._updating = true;
        for (const [key, item] of this._toggles)
            item.setToggleState(Boolean(this._settings[key]));
        for (const [key, {submenu, label}] of this._submenus) {
            const value = this._settings[key];
            if (value !== undefined) {
                const text = key === 'mode' ? ['Snow', 'Autumn leaves', 'Rain', 'Cherry blossom'][value] : `${value}%`;
                submenu.label.text = `${label}: ${text}`;
            }
        }
        this._updating = false;
    }
}
