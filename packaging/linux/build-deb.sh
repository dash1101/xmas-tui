#!/usr/bin/env bash
# Build the .deb: build-deb.sh VERSION path/to/xmas-tui OUTDIR
# Installs system-wide (dpkg always needs root); for a no-sudo install use
# scripts/get.sh instead. After installing, each user turns it on from the
# app (its banner, or Settings → Desktop), or with `xmas-tui --set-wallpaper`.
set -euo pipefail
VER="$1"; BIN="$2"; OUT="$3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PKG="$(mktemp -d)/xmas-tui_${VER}_amd64"
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/xmas-tui/scripts" "$PKG/usr/lib/systemd/user" \
         "$PKG/usr/share/applications" "$PKG/usr/share/doc/xmas-tui"
install -m 755 "$BIN" "$PKG/usr/bin/xmas-tui"
install -m 755 "$ROOT/scripts/xmas-wallpaper.sh" "$PKG/usr/share/xmas-tui/scripts/xmas-wallpaper.sh"
install -m 644 "$ROOT/config.example.toml" "$PKG/usr/share/xmas-tui/config.example.toml"
sed "s|@DIR@|/usr/share/xmas-tui|g" "$ROOT/scripts/xmas-tui.service" > "$PKG/usr/lib/systemd/user/xmas-tui.service"
install -m 644 "$ROOT/LICENSE" "$PKG/usr/share/doc/xmas-tui/copyright"
cat > "$PKG/usr/share/applications/xmas-tui.desktop" <<D
[Desktop Entry]
Type=Application
Name=xmas-tui
Comment=The Christmas village, in a window (and from there, the desktop wallpaper)
Exec=xmas-tui --windowed
Icon=xmas-tui
StartupWMClass=xmas-tui
Categories=Game;
D
cat > "$PKG/usr/share/applications/xmas-tui-settings.desktop" <<D
[Desktop Entry]
Type=Application
Name=xmas-tui settings
Comment=Settings for the xmas-tui Christmas wallpaper
Exec=xmas-tui --settings-window
Icon=xmas-tui
Categories=Settings;DesktopSettings;
D
for n in 16 24 32 48 64 128 256 512; do
  install -D -m 644 "$ROOT/assets/icon/xmas-tui-$n.png" "$PKG/usr/share/icons/hicolor/${n}x${n}/apps/xmas-tui.png"
done
chmod -R u+rwX,go+rX,go-w "$PKG"
SIZE=$(du -sk "$PKG/usr" | cut -f1)
cat > "$PKG/DEBIAN/control" <<C
Package: xmas-tui
Version: $VER
Section: x11
Priority: optional
Architecture: amd64
Installed-Size: $SIZE
Recommends: kitty (>= 0.34)
Maintainer: dash1101 <62864632+dash1101@users.noreply.github.com>
Homepage: https://github.com/dash1101/xmas-tui
Description: cozy, glowing Christmas village as an interactive live wallpaper
 A Christmas village rendered as a terminal UI and run as a live desktop
 wallpaper through kitty's background panel (KDE Plasma 6, Sway, Hyprland
 and other layer-shell compositors). Open xmas-tui from the app menu and
 click "set it up", or run: xmas-tui --set-wallpaper
C
cat > "$PKG/DEBIAN/postinst" <<'P'
#!/bin/sh
set -e
echo "xmas-tui installed. To make it the wallpaper, open xmas-tui from the app menu"
echo "and click 'set it up', or run as the desktop user (not root): xmas-tui --set-wallpaper"
P
chmod 755 "$PKG/DEBIAN/postinst"
mkdir -p "$OUT"
fakeroot dpkg-deb --build "$PKG" "$OUT/xmas-tui_${VER}_amd64.deb" >/dev/null 2>&1 || dpkg-deb --root-owner-group --build "$PKG" "$OUT/xmas-tui_${VER}_amd64.deb"
echo "built $OUT/xmas-tui_${VER}_amd64.deb"
