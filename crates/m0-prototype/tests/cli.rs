//! End-to-end CLI contract tests for the `m0-prototype` binary.
//!
//! These spawn the real binary with `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`
//! removed, pinning the offline contract: help/version output, exit 64 on
//! usage errors, and the `--report-only` JSON shape with exit 1. Exit codes
//! 2 (required-protocol failure) and 3 (degraded) need a live compositor;
//! their mapping is covered by unit tests on
//! `m0_prototype::cli::report_exit_code` instead.

use std::process::{Command, Output};

const REQUIRED_PROTOCOLS: [&str; 5] = [
    "wl_compositor",
    "wl_shm",
    "wl_seat",
    "xdg_wm_base",
    "zwlr_layer_shell_v1",
];

/// Command with the Wayland environment removed (offline contract).
fn bare_command(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_m0-prototype"));
    cmd.args(args)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET");
    cmd
}

fn run(args: &[&str]) -> Output {
    bare_command(args)
        .output()
        .expect("spawn m0-prototype binary")
}

#[test]
fn help_flag_exits_zero_and_prints_usage() {
    for flag in ["-h", "--help"] {
        let out = run(&[flag]);
        assert_eq!(out.status.code(), Some(0), "flag {flag}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(
                "Usage: m0-prototype [--report-only] [--pretty] [--list-apps] [--apps-dir DIR]"
            ),
            "unexpected help text:\n{stdout}"
        );
    }
}

#[test]
fn version_flag_exits_zero_and_prints_version_line() {
    for flag in ["-V", "--version"] {
        let out = run(&[flag]);
        assert_eq!(out.status.code(), Some(0), "flag {flag}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout,
            format!("m0-prototype {}\n", env!("CARGO_PKG_VERSION")),
            "flag {flag}"
        );
    }
}

#[test]
fn unexpected_argument_exits_64_with_hint() {
    let out = run(&["--bogus"]);
    assert_eq!(out.status.code(), Some(64));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--bogus"), "stderr:\n{stderr}");
    assert!(stderr.contains("--help"), "stderr:\n{stderr}");
}

#[test]
fn report_only_offline_emits_fail_json_and_exits_1() {
    let out = run(&["--report-only"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty(), "stderr noise: {:?}", out.stderr);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|err| panic!("stdout is not one JSON document: {err}\n{stdout}"));

    assert_eq!(value["tool"], "wayland-probe");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["overall"], "fail");
    assert_eq!(value["connection"]["status"], "unavailable");
    assert_eq!(
        value["connection"]["error"]["kind"], "no_wayland_display_env",
        "connection object: {}",
        value["connection"]
    );

    let required = value["required"].as_array().expect("required is an array");
    assert_eq!(required.len(), 5);
    assert!(
        required.iter().all(|entry| entry["status"] == "missing"),
        "offline entries must all be missing: {required:?}"
    );
    for name in REQUIRED_PROTOCOLS {
        assert!(
            required.iter().any(|entry| entry["name"] == name),
            "{name} missing from required list: {required:?}"
        );
    }
}

#[test]
fn pretty_flag_prints_multiline_json() {
    let out = run(&["--report-only", "--pretty"]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().count() > 10,
        "pretty output should be multi-line"
    );
    assert!(
        stdout.contains("\n  \""),
        "pretty output should be indented"
    );
}

#[test]
fn list_apps_prints_entries_from_override_dir() {
    let dir = std::env::temp_dir().join(format!("m0-listapps-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    std::fs::write(
        dir.join("org.m0.Editor.desktop"),
        "[Desktop Entry]\nType=Application\nName=Editor\nComment=Edit text\nExec=m0-edit %f\n",
    )
    .expect("write fixture");
    std::fs::write(
        dir.join("org.m0.Hidden.desktop"),
        "[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\nExec=x\n",
    )
    .expect("write fixture");

    let out = run(&["--list-apps", "--apps-dir", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout, "org.m0.Editor.desktop\tEditor\n",
        "NoDisplay filtered; id TAB name line"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn list_apps_falls_back_to_demo_tiles_when_index_is_empty() {
    let dir = std::env::temp_dir().join(format!("m0-listapps-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");

    let out = run(&["--list-apps", "--apps-dir", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("demo.files\tFiles\t(demo fallback)"),
        "unexpected fallback output:\n{stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apps_dir_without_value_exits_with_usage_error() {
    let out = run(&["--apps-dir"]);
    assert_eq!(out.status.code(), Some(64));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--apps-dir requires a path"),
        "unexpected stderr:\n{stderr}"
    );
}
