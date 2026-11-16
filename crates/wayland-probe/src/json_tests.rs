//! Unit tests for the report's JSON shape, failure serialization, agreement
//! with the generated protocol bindings, and the opt-in live-compositor test.

use serde_json::Value;

use crate::classify::{build_report, PROTOCOL_SPECS};
use crate::connection::{
    probe_environment, ConnectFailure, ConnectFailureKind, EnvironmentSnapshot, ProbeOutcome,
};
use crate::model::{to_compact_json, to_pretty_json, OverallStatus};
use crate::tests::{connected_info, full_globals};

// --------------------------------------------------------------- JSON form --

#[test]
fn json_report_has_stable_key_fields() {
    let report = build_report(connected_info(), &full_globals());
    let value: Value = serde_json::from_str(&to_compact_json(&report)).unwrap();
    assert_eq!(value["tool"], "wayland-probe");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["connection"]["status"], "connected");
    assert_eq!(value["connection"]["wayland_display"], "wayland-0");
    assert_eq!(value["connection"]["xdg_runtime_dir"], "/run/user/1000");
    assert_eq!(value["compositor"]["family"], "wlroots");
    assert_eq!(value["compositor"]["wl_compositor_version"], 6);
    assert_eq!(value["overall"], "pass");
    assert!(value["globals"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g["name"] == "wl_seat"));
    let required: Vec<&str> = value["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["name"].as_str())
        .collect();
    assert_eq!(required.len(), 5);
    assert!(required.contains(&"wl_compositor"));
    assert!(required.contains(&"xdg_wm_base"));
    assert!(required.contains(&"zwlr_layer_shell_v1"));
    let entry = value["required"].as_array().unwrap()[0].clone();
    assert!(entry.get("status").is_some());
    assert!(entry.get("advertised_version").is_some());
    assert!(entry.get("min_version").is_some());
    assert!(entry.get("instances").is_some());
}

#[test]
fn json_compact_is_single_line_and_pretty_is_multiline() {
    let report = build_report(connected_info(), &[]);
    let compact = to_compact_json(&report);
    assert!(!compact.contains('\n'));
    assert!(to_pretty_json(&report).contains('\n'));
    // round-trip: both forms deserialize back to the same report
    assert_eq!(
        serde_json::from_str::<crate::model::ProbeReport>(&compact).unwrap(),
        report
    );
}

#[test]
fn connection_failure_serializes_without_panicking() {
    let snapshot = EnvironmentSnapshot {
        wayland_display: None,
        xdg_runtime_dir: Some("/run/user/1000".to_string()),
        wayland_socket_set: false,
    };
    let failure = ConnectFailure::new(ConnectFailureKind::NoWaylandDisplayEnv, "");
    let report = build_report(failure.into_connection_info(&snapshot), &[]);
    assert_eq!(report.overall, OverallStatus::Fail);
    let value: Value = serde_json::from_str(&to_compact_json(&report)).unwrap();
    assert_eq!(value["connection"]["status"], "unavailable");
    assert_eq!(
        value["connection"]["error"]["kind"],
        "no_wayland_display_env"
    );
    assert!(value["connection"]["error"]["message"].is_string());
    assert_eq!(value["connection"]["wayland_display"], Value::Null);
    assert_eq!(value["overall"], "fail");
    assert_eq!(value["globals"].as_array().unwrap().len(), 0);
}

// ------------------------------------------------ generated name agreement --

/// The spec table must match the *generated bindings*, not just documentation:
/// if a protocol crate renames an interface the probe would silently report
/// `missing`. Each spec name is cross-checked against `Proxy::interface().name`.
#[test]
fn spec_names_match_generated_bindings() {
    use wayland_client::Proxy;
    use wayland_protocols::wp;
    use wayland_protocols::xdg;
    use wayland_protocols_wlr;

    let generated: &[(&str, &str)] = &[
        ("wl_compositor", <wayland_client::protocol::wl_compositor::WlCompositor as Proxy>::interface().name),
        ("wl_shm", <wayland_client::protocol::wl_shm::WlShm as Proxy>::interface().name),
        ("wl_seat", <wayland_client::protocol::wl_seat::WlSeat as Proxy>::interface().name),
        ("wl_output", <wayland_client::protocol::wl_output::WlOutput as Proxy>::interface().name),
        ("xdg_wm_base", <xdg::shell::client::xdg_wm_base::XdgWmBase as Proxy>::interface().name),
        (
            "xdg_activation_v1",
            <xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1 as Proxy>::interface().name,
        ),
        (
            "wp_fractional_scale_manager_v1",
            <wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1 as Proxy>::interface().name,
        ),
        (
            "wp_viewporter",
            <wp::viewporter::client::wp_viewporter::WpViewporter as Proxy>::interface().name,
        ),
        (
            "zwp_text_input_manager_v3",
            <wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3 as Proxy>::interface().name,
        ),
        (
            "zwp_idle_inhibit_manager_v1",
            <wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1 as Proxy>::interface().name,
        ),
        (
            "zwlr_layer_shell_v1",
            <wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1 as Proxy>::interface().name,
        ),
        (
            "zwlr_foreign_toplevel_manager_v1",
            <wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1 as Proxy>::interface().name,
        ),
    ];
    for (spec_name, binding_name) in generated {
        assert!(
            PROTOCOL_SPECS.iter().any(|s| s.name == *spec_name),
            "spec table lost protocol {spec_name}"
        );
        assert_eq!(
            spec_name, binding_name,
            "spec name drifted from generated binding"
        );
    }
}

// ---------------------------------------------------------- live (opt-in) ----

/// Real-compositor integration test: run with `cargo test -- --ignored`
/// (or when `WAYLAND_DISPLAY` is inherited). Self-skips without a compositor
/// so offline CI never fails here.
#[test]
#[ignore = "requires a running Wayland compositor"]
fn live_probe_against_real_compositor() {
    let has_display = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var_os("WAYLAND_SOCKET").is_some();
    if !has_display {
        eprintln!("skipping: no WAYLAND_DISPLAY/WAYLAND_SOCKET in environment");
        return;
    }
    match probe_environment() {
        ProbeOutcome::Connected(globals) => {
            let report = build_report(connected_info(), &globals);
            let value: Value = serde_json::from_str(&to_compact_json(&report)).unwrap();
            assert_eq!(value["connection"]["status"], "connected");
            assert!(!report.globals.is_empty());
            // a real compositor must at least offer the core triplet
            for name in ["wl_compositor", "wl_shm", "wl_seat"] {
                assert!(
                    report.globals.iter().any(|g| g.name == name),
                    "live compositor lacks {name}"
                );
            }
        }
        ProbeOutcome::Failed(failure) => {
            panic!(
                "probe failed against advertised compositor: {:?}",
                failure.kind
            )
        }
    }
}
