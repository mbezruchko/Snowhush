import importlib.util
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import time
import unittest
from unittest.mock import patch


LINUX = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


snow = load("snowhush_x11", LINUX / "gnome-x11.py")
installer = load("snowhush_installer", LINUX / "install-gnome-x11.py")


def screen(intensity):
    pointer = SimpleNamespace(get_position=lambda: (None, -1000, -1000))
    seat = SimpleNamespace(get_pointer=lambda: pointer)
    display = SimpleNamespace(get_default_seat=lambda: seat)
    return SimpleNamespace(
        geometry=SimpleNamespace(x=0, y=0, width=700, height=700),
        intensity=intensity, flakes=[], windows=[], piles={}, last=time.monotonic() - 0.04,
        get_display=lambda: display, queue_draw=lambda: None,
    )


class SnowTests(unittest.TestCase):
    def test_ten_creates_more_snow_than_two(self):
        low, high = screen(2), screen(10)
        snow.Snow.step(low, time.monotonic())
        snow.Snow.step(high, time.monotonic())
        self.assertEqual(len(low.flakes), 16)
        self.assertEqual(len(high.flakes), 80)
        self.assertEqual(int(700 * 700 / 7000 * high.intensity), 5 * int(700 * 700 / 7000 * low.intensity))

    def test_snow_settles_follows_and_drops_with_a_window(self):
        shown = screen(0)
        snow.Snow.update_windows(shown, [(4, 100, 200, 400, 500)])
        shown.flakes = [[150, 199, 58, 2, 0]]
        snow.Snow.step(shown, time.monotonic())
        self.assertEqual(len(shown.piles[4]), 1)
        snow.Snow.update_windows(shown, [(4, 160, 240, 460, 540)])
        self.assertEqual(len(shown.piles[4]), 1)
        snow.Snow.update_windows(shown, [])
        self.assertNotIn(4, shown.piles)
        self.assertTrue(any(len(flake) > 5 and flake[5] == 4 for flake in shown.flakes))


class LauncherTests(unittest.TestCase):
    def test_installer_places_a_quoted_launcher_and_preserves_existing_files(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder) / "home with spaces $"
            home.mkdir()
            with patch.dict(os.environ, {"HOME": str(home), "XDG_DATA_HOME": str(home / "share")}):
                with patch("sys.argv", ["install-gnome-x11.py"]):
                    installer.install()
                    launcher = home / "share/applications/snowhush-gnome-x11.desktop"
                    binary = home / ".local/bin/snowhush-gnome-x11"
                    self.assertTrue(binary.stat().st_mode & 0o111)
                    self.assertIn("Exec=" + installer.desktop_exec(str(binary)), launcher.read_text())
                    with self.assertRaises(SystemExit):
                        installer.install()


if __name__ == "__main__":
    unittest.main()
