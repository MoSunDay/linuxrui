//! Unit tests for the pure classification logic (offline).
//!
//! JSON-shape, connection-failure and generated-bindings tests live in
//! [`crate::json_tests`]; the socketpair fake-compositor integration test
//! lives in `tests/fake_compositor.rs`, and the real-compositor test is an
//! `#[ignore]`d unit test that self-skips without `WAYLAND_DISPLAY`.

use crate::classify::{build_report, compositor_family, dedupe_globals, Tier, PROTOCOL_SPECS};
use crate::model::{ConnectionInfo, ConnectionStatus, OverallStatus, ProtocolStatus, RawGlobal};

pub(crate) fn connected_info() -> ConnectionInfo {
    ConnectionInfo {
        status: ConnectionStatus::Connected,
        wayland_display: Some("wayland-0".to_string()),
        xdg_runtime_dir: Some("/run/user/1000".to_string()),
        error: None,
    }
}

pub(crate) fn raw(id: u32, interface: &str, version: u32) -> RawGlobal {
    RawGlobal {
        id,
        interface: interface.to_string(),
        version,
    }
}

/// A registry advertisement that satisfies every required AND recommended
/// protocol (approximates a current wlroots compositor like Sway/labwc).
pub(crate) fn full_globals() -> Vec<RawGlobal> {
    vec![
        raw(1, "wl_compositor", 6),
        raw(2, "wl_shm", 1),
        raw(3, "wl_seat", 9),
        raw(4, "xdg_wm_base", 6),
        raw(5, "zwlr_layer_shell_v1", 5),
        raw(6, "xdg_activation_v1", 1),
        raw(7, "wp_fractional_scale_manager_v1", 1),
        raw(8, "wp_viewporter", 1),
        raw(9, "zwlr_foreign_toplevel_manager_v1", 3),
        raw(10, "zwp_text_input_manager_v3", 1),
        raw(11, "wl_output", 4),
        raw(12, "zwp_idle_inhibit_manager_v1", 1),
        raw(13, "wl_data_device_manager", 3),
    ]
}

// ---------------------------------------------------------------- dedupe ----

#[test]
fn dedupe_picks_highest_version_and_counts_instances() {
    let globals = vec![
        raw(10, "wl_seat", 5),
        raw(11, "wl_seat", 8),
        raw(12, "wl_seat", 7),
        raw(20, "wl_output", 4),
        raw(21, "wl_output", 4),
    ];
    let summaries = dedupe_globals(&globals);
    assert_eq!(summaries.len(), 2);
    let seat = summaries.iter().find(|s| s.name == "wl_seat").unwrap();
    assert_eq!((seat.version, seat.instances), (8, 3));
    let output = summaries.iter().find(|s| s.name == "wl_output").unwrap();
    assert_eq!((output.version, output.instances), (4, 2));
}

#[test]
fn dedupe_drops_duplicate_registry_ids_and_sorts_by_name() {
    let globals = vec![
        raw(1, "zeta_interface", 1),
        raw(1, "zeta_interface", 9), // duplicate id: ignored even with higher version
        raw(2, "alpha_interface", 1),
    ];
    let summaries = dedupe_globals(&globals);
    assert_eq!(
        summaries
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha_interface", "zeta_interface"]
    );
    let zeta = summaries
        .iter()
        .find(|s| s.name == "zeta_interface")
        .unwrap();
    assert_eq!((zeta.version, zeta.instances), (1, 1));
}

// ----------------------------------------------------------- classification --

#[test]
fn full_advertisement_is_pass_with_empty_missing_lists() {
    let report = build_report(connected_info(), &full_globals());
    assert_eq!(report.overall, OverallStatus::Pass);
    assert!(report.missing_required.is_empty());
    assert!(report.missing_recommended.is_empty());
    assert!(report
        .required
        .iter()
        .all(|e| e.status == ProtocolStatus::Ok));
    assert!(report
        .recommended
        .iter()
        .all(|e| e.status == ProtocolStatus::Ok));
}

#[test]
fn missing_required_protocol_fails_and_is_listed() {
    let globals: Vec<RawGlobal> = full_globals()
        .into_iter()
        .filter(|g| g.interface != "zwlr_layer_shell_v1")
        .collect();
    let report = build_report(connected_info(), &globals);
    assert_eq!(report.overall, OverallStatus::Fail);
    assert_eq!(
        report.missing_required,
        vec!["zwlr_layer_shell_v1".to_string()]
    );
    let entry = report
        .required
        .iter()
        .find(|e| e.name == "zwlr_layer_shell_v1")
        .unwrap();
    assert_eq!(entry.status, ProtocolStatus::Missing);
    assert_eq!(entry.advertised_version, None);
    assert_eq!(entry.instances, 0);
}

#[test]
fn outdated_required_version_fails_like_missing() {
    let globals: Vec<RawGlobal> = full_globals()
        .into_iter()
        .map(|g| {
            if g.interface == "wl_compositor" {
                raw(g.id, &g.interface, 3)
            } else {
                g
            }
        })
        .collect();
    let report = build_report(connected_info(), &globals);
    assert_eq!(report.overall, OverallStatus::Fail);
    assert_eq!(report.missing_required, vec!["wl_compositor".to_string()]);
    let entry = report
        .required
        .iter()
        .find(|e| e.name == "wl_compositor")
        .unwrap();
    assert_eq!(entry.status, ProtocolStatus::Outdated);
    assert_eq!(entry.advertised_version, Some(3));
}

#[test]
fn missing_recommended_only_is_degraded() {
    let globals: Vec<RawGlobal> = full_globals()
        .into_iter()
        .filter(|g| g.interface != "xdg_activation_v1" && g.interface != "wp_viewporter")
        .collect();
    let report = build_report(connected_info(), &globals);
    assert_eq!(report.overall, OverallStatus::Degraded);
    assert!(report.missing_required.is_empty());
    assert_eq!(report.missing_recommended.len(), 2);
    assert!(report
        .missing_recommended
        .contains(&"xdg_activation_v1".to_string()));
    assert!(report
        .missing_recommended
        .contains(&"wp_viewporter".to_string()));
}

#[test]
fn optional_protocols_never_change_the_verdict() {
    let globals: Vec<RawGlobal> = full_globals()
        .into_iter()
        .filter(|g| g.interface != "wl_output" && g.interface != "zwp_idle_inhibit_manager_v1")
        .collect();
    let report = build_report(connected_info(), &globals);
    assert_eq!(report.overall, OverallStatus::Pass);
    assert!(report
        .optional
        .iter()
        .all(|e| e.status == ProtocolStatus::Missing));
}

#[test]
fn empty_registry_fails_with_all_required_missing() {
    let report = build_report(connected_info(), &[]);
    assert_eq!(report.overall, OverallStatus::Fail);
    assert_eq!(report.missing_required.len(), 5);
    assert!(report.globals.is_empty());
    assert_eq!(report.compositor.family, "unknown");
}

#[test]
fn compositor_family_is_heuristic_with_basis() {
    let wlroots = dedupe_globals(&[raw(1, "zwlr_layer_shell_v1", 4), raw(2, "wl_compositor", 6)]);
    let info = compositor_family(&wlroots);
    assert_eq!(
        (info.family.as_str(), info.family_basis.as_deref()),
        ("wlroots", Some("zwlr_layer_shell_v1"))
    );

    let kde = dedupe_globals(&[raw(1, "org_kde_kwin_outputmanagement", 2)]);
    assert_eq!(compositor_family(&kde).family, "kde");

    let weston = dedupe_globals(&[raw(1, "weston_desktop_shell", 1)]);
    assert_eq!(compositor_family(&weston).family, "weston");

    let plain = dedupe_globals(&full_globals());
    let info = compositor_family(&plain);
    assert_eq!(info.family, "wlroots");
    assert_eq!(info.wl_compositor_version, Some(6));
    assert_eq!(info.xdg_wm_base_version, Some(6));
}

#[test]
fn tier_split_matches_specs() {
    let report = build_report(connected_info(), &full_globals());
    assert_eq!(report.required.len(), 5);
    assert_eq!(report.recommended.len(), 5);
    assert_eq!(report.optional.len(), 2);
    for spec in PROTOCOL_SPECS {
        let entries = [&report.required, &report.recommended, &report.optional];
        assert!(
            entries
                .iter()
                .any(|v| v.iter().any(|e| e.name == spec.name)),
            "spec {} missing from report",
            spec.name
        );
    }
    assert!(PROTOCOL_SPECS.iter().any(|s| s.tier == Tier::Required));
}
