# Verification: T2.2 layer-shell panel maps and reserves its zone

- Date: 2026-11-17
- Component: `m0-panel` (wlr-layer-shell Top bar, XFCE-referenced)
- Harness: `scripts/sway-headless-panel.sh`
- Host: Ubuntu 22.04, sway 1.7, headless + pixman, unprivileged session user

## What was run

```
cargo build -p m0-panel --locked --offline
bash scripts/sway-headless-panel.sh \
  /data/caches/opencoder-target/debug/m0-panel \
  docs/evidence   # (files renamed with 0003- prefix)
```

The harness starts sway headless, launches the panel, waits for two frames,
grabs a screenshot, and audits pixels (see ADR 0003 for the check
rationale).

## Results

| Check | Result |
| --- | --- |
| Panel process alive during capture | yes |
| Bar background (#16181d) across the top edge | yes |
| Xfce-blue accent (#36689e) on the bar's bottom edge | yes |
| Text ink (label/clock) present inside the bar | yes |
| Row below the bar not painted by the panel (exclusive zone) | yes |
| Compositor tree contains `m0-panel` | no - sway 1.7 omits layer-shell surfaces from get_tree; pixel audit is the authoritative check (see ADR 0003) |
| Harness exit code | 0 (`PASS`) |

Combined session (panel + m0-prototype window in one sway session,
`0003-m0-desktop-combined.png`): the window tiles below the reserved bar,
confirming the exclusive zone end to end.

## Quality gates in the same batch

- `cargo test --workspace --locked --offline`: 15 suites green (panel adds 6
  render unit tests: clock formatting, layout fit/refusal, background/
  accent/text pixel audit, tracking-width math).
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: clean.
- `cargo fmt --all --check`: clean.

## Evidence files

- `docs/evidence/0003-m0-panel-sway-headless.png` - panel-only screenshot
- `docs/evidence/0003-m0-panel-sway-headless-tree.json` - sway tree (layer surfaces absent in 1.7)
- `docs/evidence/0003-m0-panel-sway-headless-summary.txt` - harness verdict lines
- `docs/evidence/0003-m0-panel-sway-headless-sway.log` - sway startup log
- `docs/evidence/0003-m0-desktop-combined.png` - panel + window in one session
