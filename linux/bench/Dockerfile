# Compositors to try the Linux app in without a display: Sway draws in software and runs headless, niri
# runs as a window inside it, and KWin runs its virtual backend. Fedora, so the same image runs natively
# on x86-64 and on Apple silicon.
FROM fedora:44
RUN dnf install -y --setopt=install_weak_deps=False \
      sway grim foot kwin niri mesa-dri-drivers mesa-libEGL dbus-daemon dbus-tools procps-ng libcap \
    && dnf clean all
# A realtime capability puts a compositor in secure-execution mode, where it ignores the environment set here.
RUN for compositor in /usr/bin/sway /usr/bin/kwin_wayland /usr/bin/niri; do setcap -r "$compositor" 2>/dev/null || true; done
RUN useradd -m tester
