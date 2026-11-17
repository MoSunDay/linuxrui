# ADR 0003: layer-shell top panel (T2.2) with pixel-audited verification

- Status: Accepted
- Date: 2026-11-17
- Follows: ADR 0002 (which deferred the SCTK layer-shell panel to T2.2)

## Context

ADR 0002 shipped the M0 shell as an xdg-shell window and explicitly deferred
the desktop *panel* to T2.2: a panel is a compositor-level chrome surface, not
an application window. This ADR records how T2.2 was implemented and, just as
important, how a layer surface can be *proven* under sway headless.

## Decision

1. **Panel = wlr-layer-shell Top surface.** `m0-panel` anchors
   TOP|LEFT|RIGHT, sets an exclusive zone equal to its height (42 logical
   px), no keyboard interactivity, namespace `m0-panel`, and lets the
   compositor dictate the width via configure. Built on
   smithay-client-toolkit 0.19.2 (`default-features = false`: no calloop, no
   xkbcommon) over the same wayland-client 0.31 stack wayland-probe uses.
2. **Rendering = hand-rolled software raster over wl_shm.** A 5x7 bitmap
   font (digits, uppercase, minimal punctuation), pure functions in
   `render.rs`: `paint(width, height, label, clock)` returns XRGB8888 bytes;
   chrome colors are shared with the shell UI (#16181d bar, #36689e accent).
   The bar redraws once per second (UTC clock) into a RawPool-backed buffer;
   buffer reuse is gated on the wl_buffer release event.
3. **Verification = pixel audit, not tree grep.** sway 1.7's
   `swaymsg -t get_tree` does not list layer-shell surfaces (`TREE_HIT=0` in
   the evidence), so `scripts/sway-headless-panel.sh` proves the bar through
   four independent pixel checks: bar background at the top edge, accent
   strip at the bottom edge of the bar, text ink inside the bar, and the row
   below the bar NOT painted by the panel (exclusive zone honored, windows
   are pushed down).

## Consequences

- A combined session (panel + shell window, `0003-m0-desktop-combined.png`)
  shows the shell window tiling *below* the reserved bar: the exclusive zone
  works end to end.
- The pixel audit needs PIL locally and `python3-pil` in CI; both harnesses
  stay single-script and teardown-clean.
- sway >= 1.8 reportedly lists layer surfaces in get_tree; upgrading the
  harness dependency later will let TREE_HIT become a hard assertion.
- The event loop is a deliberate 50ms dispatch-poll (no calloop): acceptable
  for a clock panel prototype; the real T2.x desktop shell will move to a
  calloop (or sctk event-loop) based loop when it gains input handling.
