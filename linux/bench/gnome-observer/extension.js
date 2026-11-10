import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const UUID = 'snowhush@mbezruchko.github.io';
const SERVICE = 'io.github.mbezruchko.Snowhush';
const wait = ms => new Promise(resolve => GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms,
    () => { resolve(); return GLib.SOURCE_REMOVE; }));
async function until(predicate, label) {
    for (let i = 0; i < 400; i++) {
        if (predicate())
            return;
        await wait(100);
    }
    throw new Error(`Timed out: ${label}`);
}
function call(service, path, iface, method, parameters = null) {
    return new Promise((resolve, reject) => Gio.DBus.session.call(service, path, iface, method,
        parameters, null, Gio.DBusCallFlags.NONE, 3000, null, (connection, result) => {
            try { resolve(connection.call_finish(result)); } catch (error) { reject(error); }
        }));
}

export default class Observer extends Extension {
    enable() {
        // GNOME temporarily disables/re-enables later extensions when an earlier one is disabled.
        if (this._started)
            return;
        this._started = true;
        this._results = [];
        // Advertise pointer capability before GTK binds wl_seat. Creating it just before a click can
        // lose the first events while GTK is still creating its wl_pointer resource.
        this._pointer = Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._run().catch(error => this._finish(error));
    }
    disable() {}
    _check(condition, label) {
        if (!condition)
            throw new Error(label);
        this._results.push(label);
    }
    async _finish(error = null) {
        if (error) {
            const stream = Gio.File.new_for_path(`${GLib.getenv('SNOWHUSH_BENCH_DIR')}/failure.png`).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
            await new Shell.Screenshot().screenshot(false, stream);
            stream.close(null);
        }
        const output = {pointer: global.get_pointer(), windows: global.get_window_actors().map(a => ({title: a.meta_window.get_title(), class: a.meta_window.get_wm_class(), pointer: a.meta_window.has_pointer(), focus: a.meta_window.has_focus(), frame: [a.meta_window.get_frame_rect().x, a.meta_window.get_frame_rect().y, a.meta_window.get_frame_rect().width, a.meta_window.get_frame_rect().height]})), passed: this._results, error: error ? `${error.message}\n${error.stack}` : null};
        GLib.file_set_contents(`${GLib.getenv('SNOWHUSH_BENCH_DIR')}/result.json`, JSON.stringify(output, null, 2));
    }
    async _run() {
        await until(() => Main.extensionManager.lookup(UUID)?.stateObj?._ready, 'Rust D-Bus bridge');
        const extension = Main.extensionManager.lookup(UUID).stateObj;
        Main.overview.hide();
        await until(() => extension._overlays.size === 1 && !Main.overview.visible, 'mapped overlay');
        const overlay = () => [...extension._overlays.keys()][0];
        let window = overlay();
        const area = Main.layoutManager.getWorkAreaForMonitor(0);
        await until(() => {
            const r = window.get_frame_rect();
            return r.x === area.x && r.y === area.y && r.width === area.width && r.height === area.height;
        }, 'overlay placement after work-area report');
        let rect = window.get_frame_rect();
        this._check(rect.x === area.x && rect.y === area.y && rect.width === area.width && rect.height === area.height,
            'Rust xdg-shell overlay fills the GNOME work area');
        this._check(window.get_window_type() === Meta.WindowType.DOCK && window.is_skip_taskbar(),
            'Trusted overlay is a dock and is excluded from the window list');
        this._check(!window.has_focus(), 'Overlay does not take keyboard focus');
        this._check(extension._client.owns_window(window), 'Mutter verifies subprocess ownership');
        const control = (key, value) => call(SERVICE, '/io/github/mbezruchko/Snowhush', `${SERVICE}.Gnome`,
            'Control', new GLib.Variant('(si)', [key, value]));
        await control('intensity', 100);
        for (let mode = 0; mode < 4; mode++) {
            await control('mode', mode);
            await wait(350);
            const result = await call(SERVICE, '/io/github/mbezruchko/Snowhush', `${SERVICE}.Gnome`, 'Settings');
            this._check(JSON.parse(result.deepUnpack()[0]).mode === mode, `Weather mode ${mode} is applied and saved`);
        }
        await control('mode', 0);
        await control('behind', 1);
        await until(() => overlay() && overlay() !== window && overlay().get_window_type() === Meta.WindowType.DESKTOP,
            'behind-windows surface replacement');
        this._results.push('Behind windows replaces the surface with a native desktop window');
        window = overlay();
        await control('behind', 0);
        await until(() => overlay() && overlay() !== window && overlay().get_window_type() === Meta.WindowType.DOCK,
            'foreground surface replacement');
        this._results.push('Returning above windows restores a native dock window');
        Main.overview.show();
        await wait(500);
        this._check(!overlay().get_compositor_private().visible, 'Overlay hides immediately in the overview');
        Main.overview.hide();
        await wait(800);
        this._check(overlay().get_compositor_private().visible, 'Overlay returns after the overview closes');
        await until(() => global.get_window_actors().some(a => a.meta_window.get_title() === 'Snowhush click-through test'), 'test application');
        const app = global.get_window_actors().find(a => a.meta_window.get_title() === 'Snowhush click-through test').meta_window;
        await until(() => app.get_compositor_private()?.mapped, 'GTK window mapped');
        app.move_resize_frame(false, 100, 120, 500, 300);
        app.activate(global.get_current_time());
        await wait(500);
        const r = app.get_frame_rect();
        const pointer = this._pointer;
        pointer.notify_absolute_motion(GLib.get_monotonic_time(), r.x + r.width / 2, r.y + r.height / 2);
        await until(() => app.has_pointer(), 'pointer focus passes through overlay');
        await wait(300);
        pointer.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED);
        await wait(100);
        pointer.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED);
        await wait(500);
        const clicks = await call(`${SERVICE}.Bench`, '/io/github/mbezruchko/Snowhush/Bench', `${SERVICE}.Bench`, 'Clicks');
        const presses = await call(`${SERVICE}.Bench`, '/io/github/mbezruchko/Snowhush/Bench', `${SERVICE}.Bench`, 'Presses');
        this._check(presses.deepUnpack()[0] >= 1, `Pointer button events pass through Rust overlay to GTK (presses=${presses.deepUnpack()[0]}, clicks=${clicks.deepUnpack()[0]})`);
        const snowStream = Gio.File.new_for_path(`${GLib.getenv('SNOWHUSH_BENCH_DIR')}/snow.png`).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        await new Shell.Screenshot().screenshot(false, snowStream);
        snowStream.close(null);
        app.make_fullscreen();
        await until(() => {
            const [ok, bytes] = GLib.file_get_contents(`${GLib.getenv('XDG_STATE_HOME')}/snowhush/snowhush.log`);
            return ok && new TextDecoder().decode(bytes).includes(`covered by ${SERVICE}.Bench`);
        }, 'fullscreen pause in Rust');
        this._results.push('Rust pauses snow when the GTK application goes fullscreen');
        window = overlay();
        await control('fullscreen', 1);
        await until(() => overlay() && overlay() !== window && overlay().get_frame_rect().height === 720,
            'overlay expands over fullscreen');
        this._check(overlay().get_layer() >= app.get_layer(), 'Show over full screen places the overlay above the fullscreen application');
        await control('fullscreen', 0);
        app.unmake_fullscreen();
        await control('clear', 1);
        await wait(1500);
        const stream = Gio.File.new_for_path(`${GLib.getenv('SNOWHUSH_BENCH_DIR')}/gnome.png`).replace(null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        await new Shell.Screenshot().screenshot(false, stream);
        stream.close(null);
        const pid = Number(extension._process.get_identifier());
        await Main.extensionManager.disableExtension(UUID);
        await until(() => !GLib.file_test(`/proc/${pid}`, GLib.FileTest.EXISTS), 'engine shutdown');
        this._results.push('Disabling the extension terminates its Rust subprocess');
        this._finish();
    }
}
