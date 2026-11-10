import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk?version=4.0';

const NAME = 'io.github.mbezruchko.Snowhush.Bench';
let clicks = 0;
let presses = 0;
let exported;
const app = new Gtk.Application({application_id: NAME});
app.connect('activate', () => {
    const window = new Gtk.ApplicationWindow({application: app, title: 'Snowhush click-through test',
        default_width: 500, default_height: 300});
    const button = new Gtk.Button({label: 'Click through the snow'});
    button.connect('clicked', () => { clicks++; });
    const gesture = new Gtk.GestureClick();
    gesture.set_propagation_phase(Gtk.PropagationPhase.CAPTURE);
    gesture.connect('pressed', () => { presses++; });
    button.add_controller(gesture);
    window.set_child(button);
    exported = Gio.DBusExportedObject.wrapJSObject(`<node><interface name="${NAME}"><method name="Clicks"><arg type="i" direction="out"/></method><method name="Presses"><arg type="i" direction="out"/></method></interface></node>`,
        {Clicks: () => clicks, Presses: () => presses});
    exported.export(app.get_dbus_connection(), '/io/github/mbezruchko/Snowhush/Bench');
    window.present();
});
app.run([]);
