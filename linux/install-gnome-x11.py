#!/usr/bin/env python3
"""Install the GNOME/X11 overlay and launcher for the current user."""

import os
from pathlib import Path
import shutil
import sys


def desktop_exec(path):
    if any(char in path for char in "\r\n"):
        raise ValueError("newline in executable path")
    escaped = path.replace("\\", "\\\\\\\\").replace('"', '\\\\\\"')
    escaped = escaped.replace("$", "\\\\$").replace("`", "\\\\`").replace("%", "%%")
    return f'"{escaped}"'


def install():
    root = Path(__file__).resolve().parent
    home = Path.home()
    data = Path(os.environ.get("XDG_DATA_HOME", home / ".local/share"))
    if not data.is_absolute():
        data = home / ".local/share"
    binary = home / ".local/bin/snowhush-gnome-x11"
    launcher = data / "applications/snowhush-gnome-x11.desktop"
    icon = data / "icons/hicolor/256x256/apps/snowhush-gnome-x11.png"
    targets = (binary, launcher, icon)
    if any(path.exists() for path in targets) and "--force" not in sys.argv[1:]:
        sys.exit("Snowhush launcher files already exist; use --force to replace them")
    entry = (root / "packaging/snowhush-gnome-x11.desktop").read_text()
    entry = entry.replace("Exec=snowhush-gnome-x11", f"Exec={desktop_exec(str(binary))}")
    for path in targets:
        path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / "gnome-x11.py", binary)
    binary.chmod(0o755)
    shutil.copyfile(root.parent / "windows/assets/icon-256.png", icon)
    launcher.write_text(entry)
    print(f"Installed {launcher}")


if __name__ == "__main__":
    if sys.argv[1:] not in ([], ["--force"]):
        sys.exit("Usage: python3 linux/install-gnome-x11.py [--force]")
    install()
