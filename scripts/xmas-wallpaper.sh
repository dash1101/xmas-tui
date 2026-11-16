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
# kitty draws it: the newest of the one on the PATH and an official build in
# ~/.local/kitty.app (Settings → Desktop → "set as wallpaper" fetches one).
# From 0.42 the panel is `kitten panel`; 0.34–0.41 (Debian 13 has 0.41) only
# have `kitty +kitten panel`, without --listen-on.
kver() { "$1" --version 2>/dev/null | awk '{print $2}'; }
at_least() { [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" == "$2" ]]; }
KITTY=""; KV=""
for k in "$(command -v kitty || true)" "$HOME/.local/kitty.app/bin/kitty"; do
  [[ -n "$k" && -x "$k" ]] || continue
  v="$(kver "$k")"; [[ -n "$v" ]] || continue
  if [[ -z "$KV" ]] || ! at_least "$KV" "$v"; then KITTY="$k"; KV="$v"; fi
done
if [[ -z "$KITTY" ]]; then
  echo "xmas-tui: kitty isn't installed (Settings → Desktop → set as wallpaper fetches it)" >&2
  sleep 30; exit 1
fi
if [[ "${XDG_SESSION_TYPE:-}" == x11 ]] && ! at_least "$KV" 0.42.0; then
  echo "xmas-tui: kitty $KV can't draw the desktop on X11 (that needs 0.42 or newer)" >&2
  sleep 30; exit 1
fi
KITTEN="${KITTEN:-$(dirname "$KITTY")/kitten}"
[[ -x "$KITTEN" ]] || KITTEN="$(command -v kitten || echo kitten)"
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
# the render scale (Settings → Display) sizes the font: read it from the
# config; the app changes it live through kitty's control socket
CONF="${XDG_CONFIG_HOME:-$HOME/.config}/xmas-tui/config.toml"
SCALE="$(sed -n 's/^scale *= *\([0-9.]*\).*/\1/p' "$CONF" 2>/dev/null | head -n1)"
BASE="${XMAS_FONT_SIZE:-10}"
FONT="$(awk -v b="$BASE" -v s="${SCALE:-1}" 'BEGIN { s = (s < 0.5 ? 0.5 : (s > 2 ? 2 : s)); printf "%.2f", b * s }')"
SOCK="${XDG_RUNTIME_DIR:-/tmp}/xmas-tui-kitty-$$"
export XMAS_KITTY_SOCKET="unix:$SOCK" XMAS_FONT_BASE="$BASE" XMAS_KITTEN="$KITTEN"
# --listen-on (not -o listen_on, which gets "-<pid>" appended) so the app
# finds the socket; kitty's own shortcuts are cleared so Ctrl(+Shift) +/-
# reach the app (zoom / resolution) instead of resizing the font directly
ARGS=(--edge=background --class xmas-tui
      -o "font_size=$FONT"
      -o allow_remote_control=socket-only
      -o clear_all_shortcuts=yes
      -o background=#03050f
      -o confirm_os_window_close=0
      -o cursor_shape=block -o cursor_blink_interval=0
      -o default_pointer_shape=arrow -o pointer_shape_when_grabbed=arrow
      -o disable_ligatures=always)
[[ -n "${XMAS_OUTPUT:-}" ]] && ARGS+=(--output-name "$XMAS_OUTPUT")
if at_least "$KV" 0.42.0; then
  exec "$KITTEN" panel "${ARGS[@]}" --focus-policy=on-demand --listen-on "unix:$SOCK" "$BIN" --wallpaper "$@"
fi
# older: kitty appends "-<pid>" to a listen_on socket (the app looks for it)
"$KITTY" +kitten panel --help 2>/dev/null | grep -q -- --focus-policy && ARGS+=(--focus-policy=on-demand)
exec "$KITTY" +kitten panel "${ARGS[@]}" -o "listen_on=unix:$SOCK" "$BIN" --wallpaper "$@"
