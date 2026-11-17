#!/usr/bin/env bash
# T2.2 verification: run m0-panel (wlr-layer-shell top bar) inside a real sway
# headless session and prove the bar is mapped at the top edge with its
# exclusive zone honored.
#
# Usage: scripts/sway-headless-panel.sh <binary> <outdir>
# Requires sway, swaymsg, grim, and python3 with PIL (pixel audit).
set -euo pipefail

BIN=${1:?usage: sway-headless-panel.sh <binary> <outdir>}
OUTDIR=${2:?usage: sway-headless-panel.sh <binary> <outdir>}
BIN=$(readlink -f "$BIN")
mkdir -p "$OUTDIR"

PANEL_H=42

RUN=/tmp/m0-panel-run-$$
mkdir -p "$RUN/xdg"
chmod 700 "$RUN/xdg"
cp "$BIN" "$RUN/panel"
cat > "$RUN/config" <<'CFG'
output HEADLESS-1 mode 1280x800
CFG

XDG="$RUN/xdg"
SOCK="$XDG/sway-ipc.sock"
DISPLAY_NAME="m0-sway-$$"

if [ "$(id -u)" -eq 0 ]; then
  id m0run >/dev/null 2>&1 || useradd -m -s /usr/sbin/nologin m0run
  chown -R m0run:m0run "$RUN" "$OUTDIR"
  as_user() {
    runuser -u m0run -- env \
      XDG_RUNTIME_DIR="$XDG" \
      WLR_BACKENDS=headless \
      WLR_LIBINPUT_NO_DEVICES=1 \
      WLR_RENDERER=pixman \
      SWAYSOCK="$SOCK" \
      WAYLAND_DISPLAY="${WL_SOCK:-$DISPLAY_NAME}" \
      "$@"
  }
else
  as_user() {
    env \
      XDG_RUNTIME_DIR="$XDG" \
      WLR_BACKENDS=headless \
      WLR_LIBINPUT_NO_DEVICES=1 \
      WLR_RENDERER=pixman \
      SWAYSOCK="$SOCK" \
      WAYLAND_DISPLAY="${WL_SOCK:-$DISPLAY_NAME}" \
      "$@"
  }
fi

cleanup() {
  kill "${PANEL_PID:-0}" 2>/dev/null || true
  as_user swaymsg -q exit 2>/dev/null || true
  sleep 1
  rm -rf "$RUN"
}
trap cleanup EXIT

as_user sway --unsupported-gpu -c "$RUN/config" >"$OUTDIR/sway.log" 2>&1 &
SWAY_PID=$!

for _ in $(seq 1 150); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
[ -S "$SOCK" ] || { echo "sway socket never appeared" | tee "$OUTDIR/summary.txt"; cat "$OUTDIR/sway.log" >&2; exit 1; }

WL_SOCK=$(ls "$XDG" | grep -m1 '^wayland-' || true)
[ -n "$WL_SOCK" ] || { echo "no wayland socket in $XDG" | tee -a "$OUTDIR/summary.txt"; exit 1; }

as_user "$RUN/panel" >"$OUTDIR/panel.log" 2>&1 &
PANEL_PID=$!

sleep 4

as_user swaymsg -t get_tree > "$OUTDIR/session-tree.json" 2>/dev/null || true
TREE_HIT=$(grep -c 'm0-panel' "$OUTDIR/session-tree.json" 2>/dev/null || true)

SHOT=no
as_user grim -o HEADLESS-1 "$OUTDIR/panel.png" 2>>"$OUTDIR/sway.log" && SHOT=yes

AUDIT=unknown
if [ "$SHOT" = yes ]; then
  AUDIT=$(python3 - "$OUTDIR/panel.png" "$PANEL_H" <<'PY'
import sys
from PIL import Image
path, panel_h = sys.argv[1], int(sys.argv[2])
im = Image.open(path).convert("RGB")
w, h = im.size
bg, accent = (22, 24, 29), (54, 104, 158)
def row_colors(y):
    return {im.getpixel((x, y)) for x in range(0, w, 8)}
checks = {
    "bg_top": bg in row_colors(5),
    "accent": accent in row_colors(panel_h - 2),
    "text_ink": any(
        c not in (bg, accent) for c in row_colors(20)
    ),
    "zone_below": bg not in row_colors(panel_h + 18),
}
verdict = "pass" if all(checks.values()) else "fail"
print(verdict + ":" + ",".join(f"{k}={v}" for k, v in checks.items()))
PY
  )
fi

BYTES=0
[ -f "$OUTDIR/panel.png" ] && BYTES=$(stat -c%s "$OUTDIR/panel.png")

{
  echo "PANEL_ALIVE=$(kill -0 "$PANEL_PID" 2>/dev/null && echo yes || echo no)"
  echo "TREE_HIT=$TREE_HIT"
  echo "SCREENSHOT=$SHOT"
  echo "PNG_BYTES=$BYTES"
  echo "PIXEL_AUDIT=$AUDIT"
} | tee "$OUTDIR/summary.txt"

case "$AUDIT" in
  pass*) echo "PASS" ;;
  *) echo "FAIL: pixel audit did not pass" >&2; exit 2 ;;
esac
