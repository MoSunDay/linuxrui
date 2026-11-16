//! Pure CLI contract of the `m0-prototype` binary.
//!
//! Argument parsing and the report-to-exit-code mapping are free functions
//! over plain data so the external contract (flags, help/version text, exit
//! codes 0/1/2/3/64) is unit-testable without a Wayland compositor.
//! `tests/cli.rs` pins the offline end-to-end behavior on top of this.

use wayland_probe::model::{OverallStatus, ProbeReport};

/// Exit code when the probe could not connect to a compositor.
pub const EXIT_CONNECT: u8 = 1;
/// Exit code when a required protocol is missing or outdated.
pub const EXIT_PROTOCOL_FAIL: u8 = 2;
/// Exit code when the connection works but recommended protocols are missing.
pub const EXIT_DEGRADED: u8 = 3;
/// Exit code for usage errors (`EX_USAGE` from sysexits.h).
pub const EXIT_USAGE: u8 = 64;

pub const HELP_TEXT: &str = "\
m0-prototype — Slint Wayland-only capability display

Runs one wayland-probe roundtrip, then opens a normal xdg-shell Slint window
using the software renderer. This prototype does not create a layer-shell
surface and does not prove IME/AT-SPI behavior.

Usage: m0-prototype [--report-only] [--pretty]
      --report-only  Print the probe JSON and exit without creating a window
      --pretty       Pretty-print JSON (only used with --report-only;
                     ignored without it)
  -h, --help         Show this help
  -V, --version      Show version

Slint 1.18.1 is triple-licensed (GPL-3.0-only OR Slint Royalty-free 2.0 OR
Slint Commercial 3.0). No license choice is made by this prototype.";

/// Flags accepted on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CliOptions {
    /// Print the probe JSON instead of opening a window.
    pub report_only: bool,
    /// Pretty-print JSON output (no effect without `report_only`).
    pub pretty: bool,
}

/// What `main` should do after argument parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliAction {
    /// Run the probe and continue with these options.
    Run(CliOptions),
    /// Print [`HELP_TEXT`] and exit 0.
    Help,
    /// Print the version line and exit 0.
    Version,
}

/// Parse `argv[1..]`; the `Err` value is the offending argument.
///
/// Help/version flags win immediately when encountered, matching the
/// historical behavior of flag scanning in order.
pub fn parse_args(args: &[String]) -> Result<CliAction, String> {
    let mut options = CliOptions::default();
    for arg in args {
        match arg.as_str() {
            "--report-only" => options.report_only = true,
            "--pretty" => options.pretty = true,
            "-h" | "--help" => return Ok(CliAction::Help),
            "-V" | "--version" => return Ok(CliAction::Version),
            other => return Err(other.to_string()),
        }
    }
    Ok(CliAction::Run(options))
}

/// Version line printed by `--version`.
pub fn version_line() -> String {
    format!("m0-prototype {}", env!("CARGO_PKG_VERSION"))
}

/// Map a probe report to the documented process exit code:
/// 0 pass, 1 connection failure, 2 required-protocol failure, 3 degraded.
pub fn report_exit_code(report: &ProbeReport) -> u8 {
    match report.overall {
        OverallStatus::Pass => 0,
        OverallStatus::Fail if report.connection.error.is_some() => EXIT_CONNECT,
        OverallStatus::Fail => EXIT_PROTOCOL_FAIL,
        OverallStatus::Degraded => EXIT_DEGRADED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_probe::model::{
        CompositorInfo, ConnectionErrorInfo, ConnectionInfo, ConnectionStatus, GlobalSummary,
        ProtocolEntry, TOOL_NAME,
    };

    fn synthetic_report(overall: OverallStatus, error_kind: Option<&str>) -> ProbeReport {
        ProbeReport {
            tool: TOOL_NAME.to_string(),
            schema_version: 1,
            connection: ConnectionInfo {
                status: if error_kind.is_some() {
                    ConnectionStatus::Unavailable
                } else {
                    ConnectionStatus::Connected
                },
                wayland_display: None,
                xdg_runtime_dir: None,
                error: error_kind.map(|kind| ConnectionErrorInfo {
                    kind: kind.to_string(),
                    message: "synthetic".to_string(),
                }),
            },
            compositor: CompositorInfo {
                family: "unknown".to_string(),
                family_basis: None,
                wl_compositor_version: None,
                xdg_wm_base_version: None,
            },
            globals: Vec::<GlobalSummary>::new(),
            required: Vec::<ProtocolEntry>::new(),
            recommended: Vec::<ProtocolEntry>::new(),
            optional: Vec::<ProtocolEntry>::new(),
            missing_required: Vec::new(),
            missing_recommended: Vec::new(),
            overall,
        }
    }

    #[test]
    fn no_arguments_runs_window_with_defaults() {
        assert_eq!(parse_args(&[]), Ok(CliAction::Run(CliOptions::default())));
    }

    #[test]
    fn flags_are_parsed_independently() {
        let report_only = "--report-only".to_string();
        assert_eq!(
            parse_args(&[report_only]),
            Ok(CliAction::Run(CliOptions {
                report_only: true,
                pretty: false
            }))
        );
        let both = ["--report-only".to_string(), "--pretty".to_string()];
        assert_eq!(
            parse_args(&both),
            Ok(CliAction::Run(CliOptions {
                report_only: true,
                pretty: true
            }))
        );
    }

    #[test]
    fn help_and_version_flags_short_and_long() {
        for flag in ["-h", "--help"] {
            assert_eq!(parse_args(&[flag.to_string()]), Ok(CliAction::Help));
        }
        for flag in ["-V", "--version"] {
            assert_eq!(parse_args(&[flag.to_string()]), Ok(CliAction::Version));
        }
    }

    #[test]
    fn help_wins_when_combined_with_other_flags() {
        let args = ["--report-only".to_string(), "-h".to_string()];
        assert_eq!(parse_args(&args), Ok(CliAction::Help));
    }

    #[test]
    fn unknown_argument_reports_the_offending_token() {
        assert_eq!(
            parse_args(&["--bogus".to_string()]),
            Err("--bogus".to_string())
        );
    }

    #[test]
    fn exit_codes_follow_the_report_contract() {
        let pass = synthetic_report(OverallStatus::Pass, None);
        assert_eq!(report_exit_code(&pass), 0);

        let connect_failure = synthetic_report(OverallStatus::Fail, Some("no_wayland_display_env"));
        assert_eq!(report_exit_code(&connect_failure), EXIT_CONNECT);

        let protocol_failure = synthetic_report(OverallStatus::Fail, None);
        assert_eq!(report_exit_code(&protocol_failure), EXIT_PROTOCOL_FAIL);

        let degraded = synthetic_report(OverallStatus::Degraded, None);
        assert_eq!(report_exit_code(&degraded), EXIT_DEGRADED);
    }

    #[test]
    fn help_text_documents_usage_and_license_notice() {
        assert!(HELP_TEXT.contains("Usage: m0-prototype [--report-only] [--pretty]"));
        assert!(HELP_TEXT.contains("ignored without it"));
        assert!(HELP_TEXT.contains("triple-licensed"));
    }

    #[test]
    fn version_line_carries_package_version() {
        assert_eq!(
            version_line(),
            format!("m0-prototype {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
