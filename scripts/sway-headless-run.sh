#!/usr/bin/env bash
# M0 prototype: run m0-prototype inside a real sway headless Wayland session and
# capture protocol-level proof that its xdg-shell window is mapped.
#
# Usage: scripts/sway-headless-run.sh <binary> <outdir>
# Requires: sway, swaymsg, grim, and an unprivileged "m0run" user (created if
# missing) because sway refuses to run as root.
# Produces in <outdir>:
#   - session-tree.json   swaymsg -t get_tree output containing our window
#   - window.png          screenshot taken while the window is mapped
#   - summary.txt         verdict lines (MAPPED=yes etc.)
set -euo pipefail

BIN=${1:?usage: sway-headless-run.sh <binary> <outdir>}
OUTDIR=${2:?usage: sway-headless-run.sh <binary> <outdir>}
BIN=$(readlink -f "$BIN")
mkdir -p "$OUTDIR"

id m0run >/dev/null 2>&1 || useradd -m -s /usr/sbin/nologin m0run

RUN=/tmp/m0-sway-run-$$
mkdir -p "$RUN/xdg"
chmod 700 "$RUN/xdg"
cp "$BIN" "$RUN/app"
cat > "$RUN/config" <<'CFG'
output HEADLESS-1 mode 1280x800
CFG
chown -R m0run:m0run "$RUN" "$OUTDIR"

XDG="$RUN/xdg"
SOCK="$XDG/sway-ipc.sock"
DISPLAY_NAME="m0-sway-$$"

# Run a command as the unprivileged user with the session environment set.
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

cleanup() {
  kill "${APP_PID:-0}" 2>/dev/null || true
  as_user swaymsg -q exit 2>/dev/null || true
  sleep 1
  rm -rf "$RUN"
}
trap cleanup EXIT

# headless+pixman never touches the GPU, but sway refuses to start on hosts
# with the proprietary nvidia driver unless told we know what we are doing.
as_user sway --unsupported-gpu -c "$RUN/config" >"$OUTDIR/sway.log" 2>&1 &
SWAY_PID=$!

for _ in $(seq 1 50); do
  [ -S "$SOCK" ] && break
  sleep 0.1
done
[ -S "$SOCK" ] || { echo "sway socket never appeared" | tee "$OUTDIR/summary.txt"; cat "$OUTDIR/sway.log" >&2; exit 1; }

# sway names its wayland socket wayland-N; discover the actual name instead of
# guessing, so clients connect to the compositor the session really created.
WL_SOCK=$(ls "$XDG" | grep -m1 '^wayland-' || true)
[ -n "$WL_SOCK" ] || { echo "no wayland socket in $XDG" | tee -a "$OUTDIR/summary.txt"; exit 1; }

as_user "$RUN/app" >"$OUTDIR/app.log" 2>&1 &
APP_PID=$!

# poll the compositor tree until our window shows up (max ~15s)
MAPPED=no
for _ in $(seq 1 150); do
  # slint/winit leaves app_id unset; identify our toplevel by its window title
  if as_user swaymsg -t get_tree 2>/dev/null | grep -q 'Rust M0 Shell'; then
    MAPPED=yes
    break
  fi
  kill -0 "$APP_PID" 2>/dev/null || break
  sleep 0.1
done

as_user swaymsg -t get_tree > "$OUTDIR/session-tree.json" 2>/dev/null || true
sleep 1

SHOT=no
if [ "$MAPPED" = yes ] && as_user grim -o HEADLESS-1 "$OUTDIR/window.png" 2>>"$OUTDIR/sway.log"; then
  SHOT=yes
fi

BYTES=0
[ -f "$OUTDIR/window.png" ] && BYTES=$(stat -c%s "$OUTDIR/window.png")

{
  echo "MAPPED=$MAPPED"
  echo "SCREENSHOT=$SHOT"
  echo "PNG_BYTES=$BYTES"
  echo "APP_ALIVE=$(kill -0 "$APP_PID" 2>/dev/null && echo yes || echo no)"
} | tee "$OUTDIR/summary.txt"

[ "$MAPPED" = yes ] || { echo 'FAIL: window never mapped' >&2; exit 2; }
[ "$SHOT" = yes ] || { echo 'FAIL: screenshot failed' >&2; exit 3; }
[ "$BYTES" -gt 5000 ] || { echo 'FAIL: screenshot suspiciously small' >&2; exit 4; }
echo "PASS"
