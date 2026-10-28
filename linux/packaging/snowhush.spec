# Packages a binary built beforehand, as release.sh does: rpmbuild -bb with _sourcedir holding the
# binary, the desktop entry, the icons and the licence.
%global debug_package %{nil}

Name:           snowhush
Version:        %{snowhush_version}
Release:        1
Summary:        Snow on your KDE Plasma desktop
License:        MIT
URL:            https://github.com/TryCoffee/Snowhush
ExclusiveArch:  x86_64

%description
Soft snow drifts across your KDE Plasma desktop, settles on the tops of your
windows and the panel, slides off when you drag them and parts around the
cursor. Needs KDE Plasma 6 on Wayland.

%install
install -Dm755 %{_sourcedir}/snowhush %{buildroot}%{_bindir}/snowhush
install -Dm644 %{_sourcedir}/snowhush.desktop %{buildroot}%{_datadir}/applications/snowhush.desktop
install -Dm644 %{_sourcedir}/icon-64.png %{buildroot}%{_datadir}/icons/hicolor/64x64/apps/snowhush.png
install -Dm644 %{_sourcedir}/icon-256.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/snowhush.png
install -Dm644 %{_sourcedir}/icon-512.png %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/snowhush.png
install -Dm644 %{_sourcedir}/LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE

%files
%license %{_datadir}/licenses/%{name}/LICENSE
%{_bindir}/snowhush
%{_datadir}/applications/snowhush.desktop
%{_datadir}/icons/hicolor/*/apps/snowhush.png
