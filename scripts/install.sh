#!/usr/bin/env bash
# Build xmas-tui, create a default config if none exists, and install a
# systemd user service that keeps it running as the desktop wallpaper —
# restarting it automatically if it ever exits (monitor sleep, crashes…).
#   ./scripts/install.sh            install + (re)start now
#   ./scripts/install.sh --remove   stop it and remove the service
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="$UNIT_DIR/xmas-tui.service"
OLD_AUTOSTART="$HOME/.config/autostart/xmas-tui.desktop"
CONF="${XDG_CONFIG_HOME:-$HOME/.config}/xmas-tui/config.toml"

if [[ "${1:-}" == "--remove" ]]; then
  systemctl --user disable --now xmas-tui.service 2>/dev/null || true
  rm -f "$UNIT" "$OLD_AUTOSTART"
  systemctl --user daemon-reload
  pkill -x xmas-tui 2>/dev/null || true
  echo "stopped xmas-tui and removed the service"
  exit 0
fi

(cd "$DIR" && cargo build --release)

mkdir -p "$(dirname "$CONF")"
[[ -f "$CONF" ]] || cp "$DIR/config.example.toml" "$CONF"

# the service replaces the old XDG autostart entry
rm -f "$OLD_AUTOSTART"
mkdir -p "$UNIT_DIR"
sed "s|@DIR@|$DIR|g" "$DIR/scripts/xmas-tui.service" > "$UNIT"
systemctl --user daemon-reload
systemctl --user enable xmas-tui.service >/dev/null 2>&1
# clear out any copy started some other way, then (re)start the service
for p in $(pgrep -x kitty); do
  tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null | grep -q 'class xmas-tui' && kill "$p" 2>/dev/null
done
pkill -x xmas-tui 2>/dev/null || true
systemctl --user restart xmas-tui.service
echo "installed: config at $CONF, service xmas-tui.service (enabled, auto-restarting)"
