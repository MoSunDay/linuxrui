# Verification: M0 shell window runs in a real (headless) compositor session

- Date: 2026-11-17
- Component: `m0-prototype` (xdg-shell window shell, XFCE-referenced)
- Harness: `scripts/sway-headless-run.sh`
- Host: Ubuntu 22.04, sway 1.7, no display, nvidia driver present, root session

## What was run

```
cargo build -p m0-prototype --locked --offline
bash scripts/sway-headless-run.sh \
  /data/caches/opencoder-target/debug/m0-prototype \
  docs/evidence   # (files renamed with 0002- prefix)
```

The harness starts sway headless (pixman renderer), launches the prototype,
polls `swaymsg -t get_tree` for the window, screenshots with `grim`, and
tears the session down.

## Results

| Check | Result |
| --- | --- |
| sway session started (headless + pixman) | yes |
| Window mapped per `swaymsg -t get_tree` (`"name": "Rust M0 Shell (XFCE-referenced)"`) | yes |
| Screenshot captured (`grim -o HEADLESS-1`) | yes, 1280x800 PNG, 120508 bytes |
| Screenshot non-blank (PIL pixel audit) | yes: 3212 distinct colors, 47.4% non-dominant pixels, dominant color is the shell's dark panel background family |
| App alive during shot | yes |
| Harness exit code | 0 (`PASS`) |

## Quality gates in the same batch

- `cargo test --workspace --locked --offline`: all suites green
  (incl. new `--list-apps` / `--apps-dir` integration tests and
  `app_index` unit tests).
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: clean.
- `cargo fmt --all --check`: clean.

## Evidence files

- `docs/evidence/0002-m0-shell-sway-headless.png` - screenshot of the mapped window
- `docs/evidence/0002-m0-shell-sway-headless-tree.json` - full sway compositor tree
- `docs/evidence/0002-m0-shell-sway-headless-summary.txt` - harness verdict lines
- `docs/evidence/0002-m0-shell-sway-headless-sway.log` - sway startup log

## Notes

- WAYLAND_DEBUG protocol traces (captured during debugging, not archived)
  showed the full xdg-shell handshake: `get_xdg_surface` -> `get_toplevel`
  -> `configure` -> `attach`/`damage_buffer`/`commit` loop.
- sway refuses to run as root and aborts on the nvidia driver check; the
  harness therefore runs the whole session as the unprivileged `m0run` user
  with `sway --unsupported-gpu` (headless+pixman never touches the GPU).
