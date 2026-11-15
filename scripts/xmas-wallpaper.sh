#!/usr/bin/env bash
# Run xmas-tui as the live desktop wallpaper: kitty's panel kitten puts a
# real terminal on the Wayland background layer (works on KDE Plasma 6,
# Sway, Hyprland — not GNOME).
#   XMAS_FONT_SIZE=10  cell size (10 → 568x75 cells on a 5120x1440 @1.15 screen)
#   XMAS_OUTPUT=DP-2   pin to one monitor (default: the primary/active one)
# Normally started by the xmas-tui systemd user service, which restarts it
# whenever it exits (e.g. when the monitor sleeps and disconnects).
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KITTEN="${KITTEN:-$(command -v kitten || echo "$HOME/.local/bin/kitten")}"
# an installed release keeps its binary in bin/ (and updates it there); a
# source checkout runs its own build; the .deb puts it on the PATH
BIN="$DIR/bin/xmas-tui"
[[ -x "$BIN" ]] || BIN="$DIR/target/release/xmas-tui"
[[ -x "$BIN" ]] || BIN="$(command -v xmas-tui || true)"

# Wait until a real monitor is connected (after deep sleep the display can
# vanish entirely; starting a panel then would attach to nothing).
monitor_connected() {
  for s in /sys/class/drm/card*-*/status; do
    case "$s" in *Writeback*) continue ;; esac
    [[ "$(cat "$s" 2>/dev/null)" == "connected" ]] && return 0
  done
  return 1
}
waited=0
until monitor_connected; do sleep 2; waited=1; done
# give KWin a moment to bring the output up after a wake
(( waited )) && sleep 4

# focus-policy on-demand: clicking the desktop gives the wallpaper keyboard
# focus (needed to type a city in settings, and for the typed easter eggs).
# confirm_os_window_close=0: when the compositor drops the surface (monitor
# unplugged/asleep) kitty must just exit, not wait on an invisible dialog.
ARGS=(panel --edge=background --class xmas-tui --focus-policy=on-demand
      -o "font_size=${XMAS_FONT_SIZE:-10}"
      -o background=#03050f
      -o confirm_os_window_close=0
      -o cursor_shape=block -o cursor_blink_interval=0
      -o default_pointer_shape=arrow -o pointer_shape_when_grabbed=arrow
      -o disable_ligatures=always)
[[ -n "${XMAS_OUTPUT:-}" ]] && ARGS+=(--output-name "$XMAS_OUTPUT")
exec "$KITTEN" "${ARGS[@]}" "$BIN" --wallpaper "$@"
