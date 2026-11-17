# 2026-11-17 T2.2 Layer-Shell Top Panel (M0 Desktop Batch)

- Commit: 0c08a99 (ci), 9a3ecc8 (feat), d478297 (docs); tags v0.1.1-m0-shell, v0.2.0-t22-layer-panel
- 本地代码根: /data2/linuxrui
- Skill: submit

## What & Why

M0 desktop previously had only an xdg-window shell (m0-prototype). T2.2 adds a
desktop-level chrome element: a layer-shell top bar (m0-panel) that reserves an
exclusive zone at the top of the screen, plus the automation needed to prove it
on a real compositor every push instead of by hand:

- `crates/m0-panel` - wlr-layer-shell client (sctk 0.19.2): Top layer surface,
  exclusive zone 42px, 5x7 bitmap-font renderer, one-second clock redraw gated
  on buffer release. Pure render layer is fully unit-tested; protocol shell is
  covered by compositor smoke (sway 1.7 `get_tree` omits layer surfaces, so
  proof is a 4-point pixel audit: bg_top / accent / text_ink / zone_below).
- `scripts/sway-headless-run.sh` - dual-mode harness (root delegates to `m0run`,
  non-root/CI runs directly) so the same script works locally and on runners.
- `scripts/sway-headless-panel.sh` - panel pixel audit + combined panel+window
  session proving the exclusive zone behaviorally (window tiles below the bar).
- `.github/workflows/ci.yml` sway-headless job: builds both binaries, runs both
  smokes on every push, uploads evidence artifacts on failure.
- Milestone tags anchoring the two CI-green baselines.

## Tests

| Suite | File | Cases | Command |
|---|---|---|---|
| m0-panel render | crates/m0-panel/src/render.rs | clock_text_formats_around_the_day (render.rs:209), layout_places_label_left_and_clock_right (:216), layout_rejects_panels_that_cannot_fit_both_texts (:225), paint_fills_background_accent_and_text (:230), paint_on_narrow_panels_still_draws_chrome (:247), text_width_counts_tracking_between_glyphs (:255) | `cargo test -p m0-panel --offline` |
| window+panel compositor smoke | scripts/sway-headless-run.sh / sway-headless-panel.sh | 4-point pixel audit, TREE_HIT recorded, combined-desktop exclusive-zone check | local: both PASS; CI job `sway-headless` steps success |

全量回归 (local, `cargo test --workspace --locked --offline`): 103 passed, 1 ignored, 0 failed (8 suites + 7 empty-bin suites ok).

## Verification & Evidence

- Gates: `cargo build --workspace --locked --offline` Finished; `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` Finished (0 warnings); `cargo fmt --all --check` clean.
- CI: run d478297 completed success; sway-headless job steps (build both, window smoke, panel smoke) all green on GitHub runners.
- Evidence: docs/evidence/0003-m0-panel-sway-headless.png (5356B, PANEL_ALIVE=yes), 0003-m0-desktop-combined.png (window below bar), summary/tree/sway.log.

## Line Count Check

- 新文件 <= 400: render.rs 265 / state.rs 245 / main.rs 146 / sway-headless-panel.sh 125 / changelog 1/1 -> 1/1 OK

## Related Docs

- docs/adr/0003-layer-shell-panel-t22.md (decision + verification rationale)
- docs/adr/0002-sway-headless-session-and-xfce-shell.md (status note: T2.2 landed)
- docs/verification/2026-11-17-t22-layer-shell-panel.md
- docs/evidence/0003-* (5 files)
- agents/<x> : N/A (repo has no agents/ directory yet)

## Retained / Not Carried

Prior window-shell batch (v0.1.1-m0-shell) remains green; this batch is purely
additive (new crate + new CI job + docs), no regression surface touched.

## Follow-Up (not blocking, tracked in review 2026-11-17)

- gate dirty redraw on buffer_free (major, next batch first item)
- explicit TEXT/DIM check in text_ink audit; PANEL_HEIGHT single-source; keep-last-width on zero-width configure
- T2.3 launcher popup (layer popup + keyboard nav, calloop loop); TREE_HIT hard assert on sway >= 1.8; app_id for window shell
