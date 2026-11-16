//! Pure conversion from a Wayland probe report to prototype UI text.

use wayland_probe::model::{
    ConnectionStatus, OverallStatus, ProbeReport, ProtocolEntry, ProtocolStatus,
};

/// Text values consumed by the Slint window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSummary {
    /// Short headline with the overall verdict.
    pub heading: String,
    /// Multi-line protocol and connection details.
    pub body: String,
}

/// Convert a report without reading environment variables or touching UI state.
pub fn summarize(report: &ProbeReport) -> ProbeSummary {
    let verdict = match report.overall {
        OverallStatus::Pass => "PASS",
        OverallStatus::Degraded => "DEGRADED",
        OverallStatus::Fail => "FAIL",
    };
    let connection = match report.connection.status {
        ConnectionStatus::Connected => "connected",
        ConnectionStatus::Unavailable => "unavailable",
    };
    let heading = format!(
        "M0 probe: {verdict}\nConnection: {connection} | Compositor: {} (advisory)",
        report.compositor.family
    );

    let mut body = String::new();
    body.push_str("Required baseline\n");
    append_entries(&mut body, &report.required);
    body.push_str("\nRecommended UX protocols\n");
    append_entries(&mut body, &report.recommended);
    body.push_str("\nOptional extras\n");
    append_entries(&mut body, &report.optional);
    body.push_str("\nUI: Slint 1.18.1, software renderer, Wayland backend. License choice is pending the M0 ADR.");
    if let Some(error) = &report.connection.error {
        body.push_str(&format!(
            "\nConnection error [{}]: {}",
            error.kind, error.message
        ));
    }

    ProbeSummary { heading, body }
}

fn append_entries(output: &mut String, entries: &[ProtocolEntry]) {
    for entry in entries {
        let state = match entry.status {
            ProtocolStatus::Ok => format!("OK v{}", entry.advertised_version.unwrap_or(0)),
            ProtocolStatus::Outdated => format!(
                "OLD v{} (needs v{})",
                entry.advertised_version.unwrap_or(0),
                entry.min_version
            ),
            ProtocolStatus::Missing => "MISSING".to_string(),
        };
        output.push_str(&format!("- {}: {state}\n", entry.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wayland_probe::classify::build_report;
    use wayland_probe::connection::{connected_info, EnvironmentSnapshot};
    use wayland_probe::model::RawGlobal;

    fn report(globals: &[(&str, u32)]) -> ProbeReport {
        let snapshot = EnvironmentSnapshot {
            wayland_display: Some("wayland-test".to_string()),
            xdg_runtime_dir: Some("/tmp/xdg".to_string()),
            wayland_socket_set: false,
        };
        let globals: Vec<RawGlobal> = globals
            .iter()
            .enumerate()
            .map(|(id, (interface, version))| RawGlobal {
                id: id as u32 + 1,
                interface: (*interface).to_string(),
                version: *version,
            })
            .collect();
        build_report(connected_info(&snapshot), &globals)
    }

    #[test]
    fn fail_report_lists_missing_required_protocols() {
        let summary = summarize(&report(&[]));
        assert!(summary.heading.contains("FAIL"));
        assert!(summary.body.contains("- wl_compositor: MISSING"));
        assert!(summary.body.contains("- zwlr_layer_shell_v1: MISSING"));
    }

    #[test]
    fn complete_report_passes_and_shows_versions() {
        let globals = [
            ("wl_compositor", 6),
            ("wl_shm", 1),
            ("wl_seat", 9),
            ("xdg_wm_base", 6),
            ("zwlr_layer_shell_v1", 5),
            ("xdg_activation_v1", 1),
            ("wp_fractional_scale_manager_v1", 1),
            ("wp_viewporter", 1),
            ("zwlr_foreign_toplevel_manager_v1", 3),
            ("zwp_text_input_manager_v3", 1),
        ];
        let summary = summarize(&report(&globals));
        assert!(summary.heading.contains("PASS"));
        assert!(summary.body.contains("- zwlr_layer_shell_v1: OK v5"));
    }

    #[test]
    fn missing_recommended_protocol_is_degraded() {
        let globals = [
            ("wl_compositor", 6),
            ("wl_shm", 1),
            ("wl_seat", 9),
            ("xdg_wm_base", 6),
            ("zwlr_layer_shell_v1", 5),
        ];
        let summary = summarize(&report(&globals));
        assert!(summary.heading.contains("DEGRADED"));
        assert!(summary.body.contains("- xdg_activation_v1: MISSING"));
    }
}
