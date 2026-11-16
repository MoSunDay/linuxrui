//! `wayland-probe` binary: parse args, run the probe, print JSON, map exit code.
//!
//! Exit codes (stable contract, documented in `--help`):
//! `0` pass · `1` connection failed · `2` required protocol missing/outdated ·
//! `3` degraded (recommended missing/outdated) · `64` usage error.
//! The JSON report is always printed on stdout, including on failure; no panic.

use std::process::ExitCode;

use wayland_probe::classify::build_report;
use wayland_probe::connection::{
    connected_info, probe_environment, EnvironmentSnapshot, ProbeOutcome,
};
use wayland_probe::model::{to_compact_json, to_pretty_json, OverallStatus};

const HELP_TEXT: &str = "\
wayland-probe — Wayland capability probe for M0 (Sway/labwc or headless testing)

Connects to the compositor from the environment ($WAYLAND_DISPLAY + $XDG_RUNTIME_DIR,
an absolute $WAYLAND_DISPLAY path, or a $WAYLAND_SOCKET fd), performs ONE
wl_display roundtrip, and prints a stable JSON report of every advertised
wl_registry global with required/recommended/missing protocol classification.

This tool ONLY probes protocol availability. It binds no interface, creates no
surface, and a passing report does NOT prove that Slint or
smithay-client-toolkit will integrate successfully on this compositor
(second-connection, frame delivery and IME behavior are separate M0 gates).

Usage: wayland-probe [--pretty]

Options:
      --pretty    Pretty-print the JSON report (default: single line)
  -h, --help      Show this help
  -V, --version   Show version

Output: a JSON object with `connection`, `compositor` (heuristic family),
`globals`, `required`, `recommended`, `optional`, `missing_required`,
`missing_recommended` and `overall`.

Exit codes:
  0  overall pass (required + recommended usable)
  1  could not connect (report still printed, connection.error explains why)
  2  required protocol missing or outdated (overall = fail)
  3  required ok but recommended missing/outdated (overall = degraded)
  64 usage error";

fn print_usage_error(arg: &str) {
    eprintln!("error: unexpected argument '{arg}'\nfor help, run: wayland-probe --help");
}

fn main() -> ExitCode {
    let mut pretty = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--pretty" => pretty = true,
            "-h" | "--help" => {
                println!("{HELP_TEXT}");
                return ExitCode::from(0);
            }
            "-V" | "--version" => {
                println!("wayland-probe {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::from(0);
            }
            other => {
                print_usage_error(other);
                return ExitCode::from(64);
            }
        }
    }

    let snapshot = EnvironmentSnapshot::capture();
    let report = match probe_environment() {
        ProbeOutcome::Connected(globals) => build_report(connected_info(&snapshot), &globals),
        ProbeOutcome::Failed(failure) => build_report(failure.into_connection_info(&snapshot), &[]),
    };

    if pretty {
        println!("{}", to_pretty_json(&report));
    } else {
        println!("{}", to_compact_json(&report));
    }

    match report.overall {
        OverallStatus::Pass => ExitCode::from(0),
        // A connection failure always reports overall=fail; the dedicated
        // exit code 1 distinguishes "could not even ask" from "asked and the
        // baseline is not there" (2) / "usable but degraded" (3).
        OverallStatus::Fail if report.connection.error.is_some() => ExitCode::from(1),
        OverallStatus::Fail => ExitCode::from(2),
        OverallStatus::Degraded => ExitCode::from(3),
    }
}
