#!/usr/bin/env bash
# Install (or reinstall) the latest xmas-tui release as the Linux desktop
# wallpaper — no Rust toolchain needed:
#
#   curl -fsSL https://raw.githubusercontent.com/dash1101/xmas-tui/main/scripts/get.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/dash1101/xmas-tui/main/scripts/get.sh | bash -s -- --remove
#
# Puts everything in ~/.local/share/xmas-tui (the app updates itself there),
# the settings in ~/.config/xmas-tui/config.toml, and runs it from a
# systemd user service that restarts it whenever it exits.
# Needs: kitty ≥ 0.36 (for `kitten panel`) and a compositor with the
# layer-shell protocol (KDE Plasma 6, Sway, Hyprland, … — not GNOME).
set -euo pipefail
REPO="dash1101/xmas-tui"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/xmas-tui"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
CONF="${XDG_CONFIG_HOME:-$HOME/.config}/xmas-tui/config.toml"
ASSET="xmas-tui-linux-x86_64"

if [[ "${1:-}" == "--remove" ]]; then
  systemctl --user disable --now xmas-tui.service 2>/dev/null || true
  rm -f "$UNIT_DIR/xmas-tui.service"
  systemctl --user daemon-reload || true
  rm -rf "$DEST"
  echo "removed xmas-tui (settings in $CONF were kept)"
  exit 0
fi

[[ "$(uname -m)" == "x86_64" ]] || { echo "sorry: release builds are x86_64 only — build from source instead"; exit 1; }
for c in curl sha256sum systemctl; do
  command -v "$c" >/dev/null || { echo "needs $c"; exit 1; }
done
# a question, answered at the keyboard (even when this script came in
# through a pipe); no keyboard: the default
ask() {
  local q="$1" def="$2" a=""
  if [[ -r /dev/tty ]]; then
    read -r -p "$q " a < /dev/tty || a=""
  fi
  a="${a:-$def}"
  [[ "$a" =~ ^[Yy] ]]
}
# the system's package manager, as root (sudo asks for the password)
pm_install() {
  if command -v apt-get >/dev/null; then sudo apt-get install -y "$@"
  elif command -v dnf >/dev/null; then sudo dnf install -y "$@"
  elif command -v pacman >/dev/null; then sudo pacman -S --needed --noconfirm "$@"
  elif command -v zypper >/dev/null; then sudo zypper install -y "$@"
  else return 1; fi
}
if ! command -v kitten >/dev/null && [[ ! -x "$HOME/.local/bin/kitten" ]] && [[ ! -x "$HOME/.local/kitty.app/bin/kitten" ]]; then
  echo "xmas-tui runs inside kitty's panel, and kitty isn't installed."
  if ask "Install kitty now (kitty's own build, into ~/.local/kitty.app — no sudo needed)? [Y/n]" y; then
    curl -fsSL https://sw.kovidgoyal.net/kitty/installer.sh | sh /dev/stdin launch=n
    mkdir -p "$HOME/.local/bin"
    for b in kitty kitten; do
      [[ -e "$HOME/.local/bin/$b" ]] || ln -s "$HOME/.local/kitty.app/bin/$b" "$HOME/.local/bin/$b"
    done
  else
    echo "install kitty (https://sw.kovidgoyal.net/kitty/binary/), then run this again."
    exit 1
  fi
fi

TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"
[[ -n "$TAG" ]] || { echo "couldn't find the latest release"; exit 1; }
echo "installing xmas-tui $TAG…"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
BASE="https://github.com/$REPO/releases/download/$TAG"
RAW="https://raw.githubusercontent.com/$REPO/$TAG"
curl -fsSL -o "$TMP/$ASSET" "$BASE/$ASSET"
curl -fsSL -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS"
(cd "$TMP" && grep " \*\?$ASSET\$" SHA256SUMS | sha256sum -c --quiet -) || { echo "checksum mismatch — not installing"; exit 1; }
curl -fsSL -o "$TMP/xmas-wallpaper.sh" "$RAW/scripts/xmas-wallpaper.sh"
curl -fsSL -o "$TMP/xmas-tui.service" "$RAW/scripts/xmas-tui.service"
curl -fsSL -o "$TMP/config.example.toml" "$RAW/config.example.toml"

mkdir -p "$DEST/bin" "$DEST/scripts" "$(dirname "$CONF")" "$UNIT_DIR"
install -m 755 "$TMP/$ASSET" "$DEST/bin/xmas-tui"
install -m 755 "$TMP/xmas-wallpaper.sh" "$DEST/scripts/xmas-wallpaper.sh"
cp "$TMP/config.example.toml" "$DEST/config.example.toml"
[[ -f "$CONF" ]] || cp "$TMP/config.example.toml" "$CONF"
sed "s|@DIR@|$DEST|g" "$TMP/xmas-tui.service" > "$UNIT_DIR/xmas-tui.service"
# the command itself (xmas-tui --set-wallpaper, --windowed…), on PATH
mkdir -p "$HOME/.local/bin"
if [[ ! -e "$HOME/.local/bin/xmas-tui" || -L "$HOME/.local/bin/xmas-tui" ]]; then
  ln -sfn "$DEST/bin/xmas-tui" "$HOME/.local/bin/xmas-tui"
fi
systemctl --user daemon-reload
systemctl --user enable xmas-tui.service >/dev/null 2>&1
systemctl --user restart xmas-tui.service
echo "done: xmas-tui $TAG is now the wallpaper (settings: $CONF; it checks for updates itself)"
# the lock screen moves with ffmpeg (a still without)
if ! command -v ffmpeg >/dev/null; then
  if ask "Install ffmpeg too, so the KDE lock screen's village moves (uses sudo)? [y/N]" n; then
    pm_install ffmpeg || pm_install ffmpeg-free || echo "couldn't install ffmpeg here: install it with the system's package manager"
  fi
fi
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "note: ~/.local/bin isn't on PATH here, so the xmas-tui command needs it added (it is on most desktops after logging in again)";;
esac
