# ADR 0001 — M0 Slint local build baseline

- Date: 2026-11-16
- Status: **Evidence only; no M0 backend selection decision.**
- Scope: `crates/m0-prototype`, the Slint 1.18.1 dependency baseline, and offline CLI behavior.

## Context

The project plan requires M0 to prove a real vertical slice on Sway and labwc before
selecting the GUI route. This machine has Rust 1.97.1 and the system Wayland client
library, but no running Wayland compositor. Therefore this note records only the
local compile, dependency, and headless CLI evidence. It must not be read as passing
the M0 common gates or as choosing Slint over GTK4.

## Implemented baseline

`m0-prototype` is a Wayland-oriented Slint binary:

- one normal `xdg-shell` window intent via Slint's `backend-winit-wayland`;
- the software renderer and accessibility feature are enabled;
- `wayland-probe` supplies one connection/registry snapshot and a stable JSON report;
- `--report-only` and `--pretty` avoid UI creation for offline and CI use;
- the Slint surface is deliberately **not** described as a layer-shell surface.

The exact UI dependency declaration is:

```toml
slint = { version = "=1.18.1", default-features = false, features = [
  "std",
  "backend-winit-wayland",
  "renderer-software",
  "accessibility",
  "compat-1-18",
] }
slint-build = { version = "=1.18.1" }
```

`compat-1-18` is required when Slint default features are disabled. The initial UI
compile also exposed that this Slint release has no public `Window.foreground`
property; per-element text colors are used instead.

## Local verification

All commands used `--locked` and the workspace-pinned Rust 1.97.1 toolchain:

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo test --workspace --locked` | **74 passed, 0 failed, 1 ignored** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | pass |
| `cargo check -p m0-prototype --locked` | pass |
| `cargo build -p m0-prototype --locked` | pass |

The ignored test is explicitly marked as requiring a live Wayland compositor.

Offline report behavior was exercised after unsetting `WAYLAND_DISPLAY` and
`WAYLAND_SOCKET`:

- `--report-only --pretty` emitted valid JSON;
- `connection.status = unavailable`;
- `connection.error.kind = no_wayland_display_env`;
- `overall = fail`;
- all five required baseline protocols were listed as missing;
- process exit status was `1`, with no stderr noise;
- `--help` and `--version` succeeded;
- an unexpected argument produced exit status `64`.

## Dependency findings

The host graph was inspected with:

```text
cargo tree --locked -p m0-prototype -e normal
cargo tree --locked -p m0-prototype -e features,normal,build
cargo tree --locked -p m0-prototype -i x11rb -e features,normal,build
```

Findings:

1. The enabled host graph is large: 680 unique packages for normal dependencies and
   713 once build dependencies are included. This is material supply-chain and
   footprint input, but no memory or startup claim is valid without the planned
   release/PSS measurements.
2. `wgpu`, `wgpu-core`, `wgpu-hal`, `naga`, `skia-safe`, `skia-bindings`, and
   `i-slint-renderer-skia` are **absent from the enabled host graph**. They appear in
   `Cargo.lock` as optional package metadata, but no such artifacts were compiled.
   `tiny-skia` is present and is distinct from Skia C++; it belongs to Slint's
   software rendering path.
3. `x11rb v0.13.2` is present. Reverse dependency analysis shows the path:
   `m0-prototype -> slint/backend-winit-wayland -> i-slint-backend-winit/wayland ->
   arboard -> x11rb`. Linux `arboard` has an unconditional `x11rb` dependency even
   when its `wayland-data-control` feature is selected. This is a transitive
   clipboard dependency, not an enabled winit X11 backend.
4. Runtime backend selection was checked by setting `DISPLAY=:99` while unsetting the
   Wayland variables. The binary exited `1` and winit reported that `winit/x11` was
   not enabled. This is local evidence that the UI does not fall back to X11, but it
   is not a substitute for compositor integration tests.

The large lockfile entry set means future SBOM work must distinguish “locked optional
package metadata” from “packages actually enabled/compiled for the release target.”

## License and footprint consequences

- Slint 1.18.1 is offered under `GPL-3.0-only OR
  LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`; the choice
  remains open.
- The repository's own license is **MIT** for all original workspace code
  (maintainer-confirmed 2026-11-16; text in `LICENSE` at the workspace root).
  That covers original code only: it does **not** resolve distribution terms
  for binaries linked with Slint and does not satisfy the planned
  NOTICE/royalty-free disclosure review.
- No release binary size, PSS, wakeup, or startup measurement has been taken; the
  272 MB local artifact is a debug build with debug info and is not a footprint data
  point.

## Still-open M0 gates

This baseline does not close the M0 hard gates. The following require fixed Sway and
labwc sessions and remain blocking:

- one event-loop owner and one Wayland connection owner in the same executable;
- zbus/headless core crates working concurrently inside that same executable
  (part of the M0 vertical-slice definition in the plan);
- Slint plus SCTK single-connection layer-shell integration;
- real normal-window and layer-shell display/configure/destroy behavior;
- keyboard navigation, focus, and overlay interactivity;
- Chinese IME (`text-input-v3`) preedit/commit/cancel/cursor-rectangle matrix;
- Orca/AT-SPI role/name/state/list/focus behavior;
- integer/fractional scaling, mixed DPI, outputs, and hotplug;
- clipboard lifecycle and UI non-blocking behavior;
- release build, PSS/startup/wakeup budgets, and native-library/SBOM review;
- final Slint-vs-GTK4 decision ADR after both routes face the same gates.

Accordingly, checklist items 1 and 2 in `plans/rust-lightweight-linux-gui.md` remain
unchecked: the toolchain/build baseline exists, but the license and real-compositor
M0 evidence are not complete.

## Follow-up (2026-11-16, same day): local hardening

A read-only review of the baseline surfaced local evidence and regression gaps.
They were closed without changing this ADR's status or shrinking the open-gate
list above:

- the binary now reuses `m0_prototype::summary` from the lib target; the
  duplicated bin-side summary tests are gone (the bin target runs 0 tests);
- argument parsing and the report-to-exit-code mapping moved into
  `m0_prototype::cli` as pure functions with unit tests (exit codes 0/1/2/3
  and usage-error 64, help/version precedence, flag combinations);
- `tests/cli.rs` spawns the real binary with `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`
  removed and pins the offline contract: help/version output, exit 64 on
  unknown arguments, and `--report-only` JSON keys (`tool`, `schema_version`,
  `connection.error.kind = no_wayland_display_env`, five `missing` required
  entries) with exit 1 and no stderr noise;
- the Refresh button no longer probes on the UI thread: the probe runs on a
  worker thread and posts the summary back through
  `slint::invoke_from_event_loop` (a `slint::Weak` handle is the event-loop
  proxy; a late result after window drop is discarded);
- the protocol detail text sits in a `ScrollView` whose `content-width` tracks
  `visible-width`, so wrapped details scroll vertically instead of clipping;
- `wayland-probe` gained `probe_with_snapshot`; `m0-prototype` now samples the
  environment exactly once per report (no double capture);
- `--pretty` without `--report-only` is documented as ignored in `--help`;
- the workspace manifest comment now names all member groups (headless core,
  probe, M0 prototype), matching the actual workspace;
- `LICENSE` (MIT) added at the workspace root per maintainer confirmation.

Re-verification after the change (same toolchain and `--locked`):

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo test --workspace --locked` | **84 passed, 0 failed, 1 ignored** |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | pass |
| manual offline spot-check | `--report-only` exit 1 with the recorded JSON shape; unknown argument exit 64 |

None of this closes a Still-open M0 gate: the zbus/headless-core concurrency
gate added above is now tracked explicitly, and the license/footprint
consequences are unchanged.
