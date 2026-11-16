//! M0 Slint prototype entry point.
//!
//! This binary is deliberately separate from `wayland-probe`: it creates one
//! normal `xdg-shell` window through Slint's Wayland-only winit backend and
//! feeds it the probe report. It does **not** claim layer-shell, SCTK, IME or
//! accessibility success; those require the M0 gate matrix on Sway/labwc.
//!
//! Refresh never probes on the UI thread: the probe runs on a worker thread
//! and posts the summary back through `slint::invoke_from_event_loop`, so the
//! single UI event-loop owner stays responsive during the Wayland roundtrip.

use std::process::ExitCode;

use m0_prototype::cli::{self, CliAction};
use m0_prototype::summary;
use wayland_probe::classify::build_report;
use wayland_probe::connection::{
    connected_info, probe_with_snapshot, EnvironmentSnapshot, ProbeOutcome,
};
use wayland_probe::model::{to_compact_json, to_pretty_json, ProbeReport};

slint::include_modules!();

fn collect_report() -> ProbeReport {
    // One snapshot feeds validation, the connection attempt, and the report,
    // so the environment sample cannot disagree with the probe outcome.
    let snapshot = EnvironmentSnapshot::capture();
    match probe_with_snapshot(&snapshot) {
        ProbeOutcome::Connected(globals) => build_report(connected_info(&snapshot), &globals),
        ProbeOutcome::Failed(failure) => build_report(failure.into_connection_info(&snapshot), &[]),
    }
}

fn apply_summary(window: &AppWindow, report: &ProbeReport) {
    let text = summary::summarize(report);
    window.set_status_text(text.heading.as_str().into());
    window.set_protocol_text(text.body.as_str().into());
}

/// Probe off the UI thread, then apply the summary from the event loop.
///
/// The handle is a `slint::Weak` event-loop proxy: if the window is gone
/// before the probe finishes, the upgrade fails and the result is dropped.
fn refresh_in_background(window: &AppWindow) {
    let weak = window.as_weak();
    window.set_status_text("Refreshing...".into());
    std::thread::spawn(move || {
        let report = collect_report();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = weak.upgrade() {
                apply_summary(&window, &report);
            }
        });
    });
}

fn run_window(report: ProbeReport) -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    apply_summary(&window, &report);

    let refresh_handle = window.as_weak();
    window.on_refresh_clicked(move || {
        if let Some(window) = refresh_handle.upgrade() {
            refresh_in_background(&window);
        }
    });
    window.on_quit_clicked(|| {
        let _ = slint::quit_event_loop();
    });

    window.run()
}

fn main() -> ExitCode {
    let action = match cli::parse_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(action) => action,
        Err(arg) => {
            eprintln!("error: unexpected argument '{arg}'; try --help");
            return ExitCode::from(cli::EXIT_USAGE);
        }
    };
    let options = match action {
        CliAction::Help => {
            println!("{}", cli::HELP_TEXT);
            return ExitCode::from(0);
        }
        CliAction::Version => {
            println!("{}", cli::version_line());
            return ExitCode::from(0);
        }
        CliAction::Run(options) => options,
    };

    let report = collect_report();
    if options.report_only {
        if options.pretty {
            println!("{}", to_pretty_json(&report));
        } else {
            println!("{}", to_compact_json(&report));
        }
        return ExitCode::from(cli::report_exit_code(&report));
    }

    match run_window(report) {
        Ok(()) => ExitCode::from(0),
        Err(err) => {
            eprintln!("failed to run Wayland Slint prototype: {err}");
            eprintln!("verify $WAYLAND_DISPLAY/$XDG_RUNTIME_DIR and run wayland-probe first");
            ExitCode::from(1)
        }
    }
}
