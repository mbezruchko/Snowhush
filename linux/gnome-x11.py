#!/usr/bin/env python3
"""Small, click-through snow overlay for GNOME on X11 (GTK 3 + Cairo)."""

import os

# Do not accidentally create a Wayland window in a mixed session.
os.environ["GDK_BACKEND"] = "x11"

import math
import random
import sys
import time

import cairo
import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("Wnck", "3.0")
from gi.repository import Gdk, GLib, Gtk, Wnck


class Snow(Gtk.Window):
    def __init__(self, geometry, intensity):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.geometry = geometry
        self.intensity = intensity
        self.flakes = []
        self.windows = []
        self.piles = {}
        self.last = time.monotonic()
        self.set_decorated(False)
        self.set_accept_focus(False)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_type_hint(Gdk.WindowTypeHint.DOCK)
        self.set_keep_above(True)
        self.set_app_paintable(True)
        self.set_default_size(geometry.width, geometry.height)
        self.move(geometry.x, geometry.y)
        self.connect("draw", self.draw_snow)
        self.connect("map-event", self.click_through)

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual is None or not screen.is_composited():
            raise RuntimeError("An X11 compositor with RGBA windows is required")
        self.set_visual(visual)
        self.show_all()

    def click_through(self, _window, _event):
        # Empty input shape lets mouse clicks, scrolling and dragging reach the desktop.
        GLib.idle_add(self.empty_input_region)

    def empty_input_region(self):
        self.get_window().input_shape_combine_region(cairo.Region(), 0, 0)
        return False

    def update_windows(self, windows):
        old = {window[0]: window for window in self.windows}
        current = {window[0]: window for window in windows}
        for xid, grains in list(self.piles.items()):
            previous = old.get(xid)
            next_window = current.get(xid)
            if next_window is None:
                if previous:
                    self.flakes.extend([[previous[1] + x, previous[2] - level, 60, radius, 0, xid] for x, radius, level in grains])
                del self.piles[xid]
            elif previous and len(grains) > 3 and abs(next_window[1] - previous[1]) + abs(next_window[2] - previous[2]) > 25:
                # A quick drag shakes some of the drift off the window.
                fallen = grains[::3]
                self.flakes.extend([[previous[1] + x, previous[2] - level, 65, radius, 0, xid] for x, radius, level in fallen])
                self.piles[xid] = [grain for index, grain in enumerate(grains) if index % 3]
        self.windows = windows

    def step(self, now):
        dt = min(now - self.last, 0.05)
        self.last = now
        width, height = self.geometry.width, self.geometry.height
        target = int(width * height / 7000 * self.intensity)
        for _ in range(min(int(8 * max(1, self.intensity)), max(0, target - len(self.flakes)))):
            self.flakes.append([
                random.uniform(0, width), random.uniform(-20, 0),
                random.uniform(18, 58), random.uniform(0.8, 2.5), random.uniform(0, 6.28),
            ])
        pointer = self.get_display().get_default_seat().get_pointer()
        _, px, py = pointer.get_position()
        px -= self.geometry.x
        py -= self.geometry.y
        for xid, left, top, right, _bottom in self.windows:
            grains = self.piles.get(xid)
            if grains and abs(py - top) < 25:
                remaining = []
                for x, radius, level in grains:
                    if abs(px - left - x) < 22:
                        self.flakes.append([left + x, top - level, 55, radius, random.uniform(0, 6.28), xid])
                    else:
                        remaining.append([x, radius, level])
                self.piles[xid] = remaining
        falling = []
        for flake in self.flakes:
            previous_y = flake[1]
            flake[4] += dt * 1.5
            flake[0] += math.sin(flake[4]) * dt * 12
            flake[1] += flake[2] * dt
            dx, dy = flake[0] - px, flake[1] - py
            if 0 < dx * dx + dy * dy < 900:
                flake[0] += (dx / 30) * dt * 75
                flake[1] += (dy / 30) * dt * 75
            landed = False
            for index in range(len(self.windows) - 1, -1, -1):
                xid, left, top, right, _bottom = self.windows[index]
                if len(flake) > 5 and flake[5] == xid:
                    continue
                if previous_y <= top <= flake[1] and left + flake[3] <= flake[0] <= right - flake[3] and top >= 8:
                    if any(a <= flake[0] <= c and b <= top <= d for _, a, b, c, d in self.windows[index + 1:]):
                        continue
                    grains = self.piles.setdefault(xid, [])
                    if len(grains) < min(250, (right - left) // 3):
                        x = flake[0] - left
                        level = flake[3] + min(12, sum(2 * radius for other, radius, _ in grains if abs(other - x) < 3))
                        grains.append([x, flake[3], level])
                    landed = True
                    break
            if not landed and flake[1] < height + 5:
                falling.append(flake)
        self.flakes = falling
        if len(self.flakes) > target:
            del self.flakes[:len(self.flakes) - target]
        self.queue_draw()

    def draw_snow(self, _widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)
        for flake in self.flakes:
            x, y, _, radius, _ = flake[:5]
            cr.set_source_rgba(1, 1, 1, 0.8)
            cr.arc(x, y, radius, 0, 2 * math.pi)
            cr.fill()
        for index, (xid, left, top, _right, _bottom) in enumerate(self.windows):
            cr.set_source_rgba(1, 1, 1, 0.94)
            for x, radius, level in self.piles.get(xid, []):
                if any(a <= left + x <= c and b <= top - level <= d for _, a, b, c, d in self.windows[index + 1:]):
                    continue
                cr.arc(left + x, top - level, radius, 0, 2 * math.pi)
                cr.fill()
        return False


class App:
    def __init__(self):
        Wnck.set_client_type(Wnck.ClientType.PAGER)
        self.tracker = Wnck.Screen.get_default()
        self.intensity = 1.0
        self.screens = []
        self.layout = None
        self.controls = Gtk.Window(title="Snowhush · GNOME X11")
        self.controls.set_default_size(260, 90)
        self.controls.set_keep_above(True)
        self.controls.connect("destroy", Gtk.main_quit)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.set_border_width(12)
        box.pack_start(Gtk.Label(label="Snow intensity"), False, False, 0)
        scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, 10, 0.1)
        scale.set_value(self.intensity)
        scale.connect("value-changed", self.set_intensity)
        box.pack_start(scale, False, False, 0)
        self.controls.add(box)

        self.tray = Gtk.StatusIcon.new_from_icon_name("weather-snow")
        self.tray.set_tooltip_text("Snowhush · click for controls")
        self.tray.connect("activate", lambda *_: self.controls.present())
        self.tray.connect("popup-menu", self.tray_menu)
        self.sync_screens()
        self.controls.show_all()
        self.refresh_windows()
        GLib.timeout_add(100, self.refresh_windows)
        GLib.timeout_add(33, self.tick)

    def refresh_windows(self):
        self.tracker.force_update()
        workspace = self.tracker.get_active_workspace()
        windows = []
        for window in self.tracker.get_windows_stacked():
            if (window.get_pid() == os.getpid() or window.is_minimized()
                    or window.get_window_type() not in (Wnck.WindowType.NORMAL, Wnck.WindowType.DIALOG)
                    or (workspace is not None and not window.is_visible_on_workspace(workspace))):
                continue
            x, y, width, height = window.get_geometry()
            if width > 0 and height > 0:
                windows.append((window.get_xid(), x, y, x + width, y + height))
        for screen in self.screens:
            screen.update_windows([
                (xid, left - screen.geometry.x, top - screen.geometry.y, right - screen.geometry.x, bottom - screen.geometry.y)
                for xid, left, top, right, bottom in windows
                if left < screen.geometry.x + screen.geometry.width and right > screen.geometry.x
            ])
        return True

    def set_intensity(self, scale):
        self.intensity = scale.get_value()
        for screen in self.screens:
            screen.intensity = self.intensity

    def tray_menu(self, _icon, button, activation_time):
        menu = Gtk.Menu()
        for label, action in (("Controls", self.controls.present), ("Quit", Gtk.main_quit)):
            item = Gtk.MenuItem(label=label)
            item.connect("activate", lambda _item, callback=action: callback())
            menu.append(item)
        menu.show_all()
        menu.popup(None, None, Gtk.StatusIcon.position_menu, self.tray, button, activation_time)

    def sync_screens(self):
        display = Gdk.Display.get_default()
        count = display.get_n_monitors()
        layout = []
        for i in range(count):
            rect = display.get_monitor(i).get_geometry()
            layout.append((rect.x, rect.y, rect.width, rect.height))
        layout = tuple(layout)
        if layout == self.layout:
            return
        for screen in self.screens:
            screen.destroy()
        self.layout = layout
        self.screens = [Snow(display.get_monitor(i).get_geometry(), self.intensity) for i in range(count)]

    def tick(self):
        self.sync_screens()
        now = time.monotonic()
        for screen in self.screens:
            screen.step(now)
        return True


if __name__ == "__main__":
    if os.environ.get("XDG_SESSION_TYPE") == "wayland":
        sys.exit("Snowhush GNOME/X11 requires an Xorg session")
    if not Gtk.init_check()[0]:
        sys.exit("Snowhush requires a running X11 desktop")
    try:
        App()
    except RuntimeError as error:
        sys.exit(str(error))
    Gtk.main()
