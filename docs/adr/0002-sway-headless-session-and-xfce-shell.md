# ADR 0002: sway headless session verification and the XFCE-referenced xdg-shell window

- Status: Accepted
- Date: 2026-11-17
- Supersedes: none; extends ADR 0001 (slint local-build baseline)

## Context

M0 promised an "actually runnable" shell UI, not just a binary that compiles.
The host has no usable display session (headless server, proprietary nvidia
driver present, root-only shell), so "run it and see" needs a reproducible
compositor harness. Two design points needed locking down at the same time:

1. Which shell surface protocol to use for the M0 prototype window.
2. How to prove, offline and repeatably, that the window really maps.

## Decision

1. **Window shell = xdg-shell toplevel.** The M0 prototype is a normal
   application window (`Rust M0 Shell (XFCE-referenced)`), XFCE-referenced in
   layout: top panel (clock + launcher note), appfinder-style application list
   fed by `desktop-entry` scanning with `launch-core` previews, and a detail
   pane. The SCTK layer-shell panel is deliberately deferred to T2.2; a
   layer-shell panel is a real desktop-shell surface, which belongs to the
   next milestone, not to the M0 "runnable window" goal.
2. **Verification harness = `scripts/sway-headless-run.sh`.** One
   self-contained script that:
   - starts sway 1.7 with `WLR_BACKENDS=headless`, `WLR_RENDERER=pixman`,
     `WLR_LIBINPUT_NO_DEVICES=1` (no GPU, no input devices needed);
   - runs sway as the unprivileged `m0run` user (sway refuses to run as
     root), with `--unsupported-gpu` because the host's nvidia driver check
     would otherwise abort startup even though headless+pixman never touches
     the GPU;
   - discovers the actual `wayland-N` socket name instead of guessing;
   - identifies our toplevel by window title (slint/winit leaves `app_id`
     unset), polls `swaymsg -t get_tree` until the window node appears;
   - captures `grim` screenshot + the full compositor tree, and fails with
     distinct exit codes when the window never maps, the shot fails, or the
     PNG is suspiciously small.

## Consequences

- Evidence is a compositor tree node plus a pixel-checked screenshot, not a
  "process was alive" claim. See `docs/verification/2026-11-17-m0-shell-sway-headless.md`
  and `docs/evidence/0002-m0-shell-sway-headless.*`.
- The harness is CI-portable in principle but M0 CI stays build/test/clippy
  only; compositor smoke runs stay a manual (or later-stage) gate.
- `app_id` remains unset on the toplevel (winit does not forward one through
  the slint window API we use); title-based matching is the documented
  workaround and setting a proper `app_id` lands with the T2.2 shell work.
- The xdg-shell choice keeps M0 honest about scope: a desktop *panel* needs
  layer-shell; a desktop *window* does not.
