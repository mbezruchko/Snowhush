# Packages the files package.sh lays out, with snowhush_stage pointing at them: rpmbuild -bb from that script.
%global debug_package %{nil}

Name:           snowhush
Version:        %{snowhush_version}
Release:        1
Summary:        Snow on your Wayland desktop
License:        MIT
URL:            https://github.com/mbezruchko/Snowhush
ExclusiveArch:  x86_64
# Loaded while running, so the automatic requirements miss it.
Requires:       libEGL.so.1()(64bit)

%description
Soft snow drifts across your desktop, settles on the tops of your windows and
the panel, slides off when you drag them and parts around the cursor. Needs
KDE Plasma 6, Sway or niri on Wayland.

%install
cp -a %{snowhush_stage}/usr %{buildroot}/

%files
%license %{_datadir}/licenses/%{name}/LICENSE
%{_bindir}/snowhush
%{_datadir}/applications/snowhush.desktop
%{_datadir}/icons/hicolor/*/apps/snowhush.png
