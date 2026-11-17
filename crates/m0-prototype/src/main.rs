//! M0 Slint shell entry point (XFCE-referenced).
//!
//! This binary is deliberately separate from `wayland-probe`: it creates one
//! normal `xdg-shell` window through Slint's Wayland-only winit backend and
//! renders a small desktop shell referencing XFCE: an xfce4-panel-style top
//! bar with a live clock, an appfinder-style launcher list built from real
//! XDG desktop files (parsed by `desktop-entry`, launch-previewed through
//! `launch-core`), and a detail pane carrying the capability-probe report.
//! It does **not** claim layer-shell, SCTK, IME or accessibility success;
//! those require the M0 gate matrix on Sway/labwc.
//!
//! Ownership stays single-owner: the app index is built before the event
//! loop starts, the probe refreshes on a worker thread and posts results
//! back through `slint::invoke_from_event_loop`, so the one UI event-loop
//! owner is never nested.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use m0_prototype::app_index::{self, AppEntry};
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

/// Launcher roots: an explicit `--apps-dir` wins; otherwise XDG env roots.
fn launcher_roots(apps_dir: Option<&str>) -> Vec<PathBuf> {
    match apps_dir {
        Some(dir) => vec![PathBuf::from(dir)],
        None => app_index::env_roots(),
    }
}

/// Build the launcher index; the returned bool flags that fallback demo
/// tiles are in play (no real desktop files were found anywhere).
fn build_index(apps_dir: Option<&str>) -> (Vec<AppEntry>, bool) {
    let scanned = app_index::scan_roots(&launcher_roots(apps_dir));
    let current_desktops = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|raw| desktop_entry::parse_current_desktops(Some(&raw)))
        .unwrap_or_default();
    let path = std::env::var("PATH").ok();
    let real = app_index::index_visible(&scanned, &current_desktops, path.as_deref());
    if real.is_empty() {
        (app_index::fallback_entries(), true)
    } else {
        (real, false)
    }
}

fn shell_tile(entry: &AppEntry) -> AppEntryData {
    AppEntryData {
        name: entry.name.clone().into(),
        comment: entry.comment.clone().into(),
        desktop_id: entry.desktop_id.clone().into(),
        exec_preview: entry.exec_preview.clone().into(),
        terminal: entry.terminal,
        demo: entry.demo,
    }
}

fn apply_detail(window: &AppWindow, entry: &AppEntry) {
    window.set_detail_name(entry.name.clone().into());
    window.set_detail_comment(entry.comment.clone().into());
    window.set_detail_id(entry.desktop_id.clone().into());
    window.set_detail_exec(entry.exec_preview.clone().into());
    window.set_detail_terminal(entry.terminal);
    window.set_detail_demo(entry.demo);
}

/// Panel clock in UTC (M0 keeps out of TZ databases; the label says UTC).
fn utc_clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rest / 3_600, (rest % 3_600) / 60, rest % 60);
    // civil_from_days (Howard Hinnant) for the date part.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn run_window(report: ProbeReport, apps_dir: Option<&str>) -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    apply_summary(&window, &report);

    // Launcher index and panel note are applied before the event loop runs.
    let (entries, fallback) = build_index(apps_dir);
    let model = slint::VecModel::from(entries.iter().map(shell_tile).collect::<Vec<_>>());
    window.set_app_model(std::rc::Rc::new(model).into());
    window.set_launcher_note(
        if fallback {
            format!("{} tiles · fallback demo", entries.len())
        } else {
            format!("{} tiles · XDG index", entries.len())
        }
        .into(),
    );

    // Panel clock: one repeating timer owned by the single event loop.
    let clock_timer = slint::Timer::default();
    {
        let weak = window.as_weak();
        let update_clock = move || {
            if let Some(window) = weak.upgrade() {
                window.set_clock_text(utc_clock().into());
            }
        };
        update_clock();
        clock_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            update_clock,
        );
    }

    // First tile starts selected; clicks re-apply the detail pane.
    let entries = Arc::new(entries);
    if let Some(first) = entries.first() {
        apply_detail(&window, first);
        window.set_selected_index(0);
    }
    {
        let weak = window.as_weak();
        let entries = Arc::clone(&entries);
        window.on_app_selected(move |index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if let Some(entry) = entries.get(index.max(0) as usize) {
                apply_detail(&window, entry);
                window.set_selected_index(index);
            }
        });
    }

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

    // The launcher index needs no compositor: `--list-apps` is fully offline.
    if options.list_apps {
        let (entries, fallback) = build_index(options.apps_dir.as_deref());
        for entry in &entries {
            let marker = if fallback { "\t(demo fallback)" } else { "" };
            println!("{}\t{}{}", entry.desktop_id, entry.name, marker);
        }
        return ExitCode::from(0);
    }

    let report = collect_report();
    if options.report_only {
        if options.pretty {
            println!("{}", to_pretty_json(&report));
        } else {
            println!("{}", to_compact_json(&report));
        }
        return ExitCode::from(cli::report_exit_code(&report));
    }

    match run_window(report, options.apps_dir.as_deref()) {
        Ok(()) => ExitCode::from(0),
        Err(err) => {
            eprintln!("failed to run Wayland Slint prototype: {err}");
            eprintln!("verify $WAYLAND_DISPLAY/$XDG_RUNTIME_DIR and run wayland-probe first");
            ExitCode::from(1)
        }
    }
}
