//! Pure protocol classification: globals in, support report out.
//!
//! No I/O and no Wayland types here — [`build_report`] is a pure function of
//! its inputs, which is what makes the M0 probe unit-testable without a
//! compositor. Interface name strings are constants; the test suite asserts
//! they match the generated bindings of `wayland-client` / `wayland-protocols`
//! / `wayland-protocols-wlr`.

use crate::model::{
    CompositorInfo, GlobalSummary, OverallStatus, ProbeReport, ProtocolEntry, ProtocolStatus,
    RawGlobal, REPORT_SCHEMA_VERSION, TOOL_NAME,
};

/// How much the project depends on a protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Baseline: the desktop components cannot ship without it.
    Required,
    /// Full UX (activation, HiDPI, taskbar, IME); missing means degraded.
    Recommended,
    /// Reported for visibility only; never affects the overall status.
    Optional,
}

/// One protocol this project cares about, with its accepted minimum version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolSpec {
    /// Interface name exactly as it appears in `wl_registry.global`.
    pub name: &'static str,
    /// Dependency tier.
    pub tier: Tier,
    /// Minimum advertised version accepted as usable by the first release.
    ///
    /// Rationale: `wl_compositor` v4 (buffer-coordinate damage, SCTK baseline),
    /// `wl_seat` v5 (seat release; modern compositor baseline), everything
    /// else is v1-only or usable from v1.
    pub min_version: u32,
}

/// Interface names this probe classifies, in report order.
///
/// `wl_output` and `zwp_idle_inhibit_manager_v1` are tracked as `optional`:
/// a panel keeps working without them (fallback layout / no idle-inhibit).
pub const PROTOCOL_SPECS: &[ProtocolSpec] = &[
    // --- required baseline ---
    ProtocolSpec {
        name: "wl_compositor",
        tier: Tier::Required,
        min_version: 4,
    },
    ProtocolSpec {
        name: "wl_shm",
        tier: Tier::Required,
        min_version: 1,
    },
    ProtocolSpec {
        name: "wl_seat",
        tier: Tier::Required,
        min_version: 5,
    },
    ProtocolSpec {
        name: "xdg_wm_base",
        tier: Tier::Required,
        min_version: 1,
    },
    ProtocolSpec {
        name: "zwlr_layer_shell_v1",
        tier: Tier::Required,
        min_version: 1,
    },
    // --- recommended (full UX) ---
    ProtocolSpec {
        name: "xdg_activation_v1",
        tier: Tier::Recommended,
        min_version: 1,
    },
    ProtocolSpec {
        name: "wp_fractional_scale_manager_v1",
        tier: Tier::Recommended,
        min_version: 1,
    },
    ProtocolSpec {
        name: "wp_viewporter",
        tier: Tier::Recommended,
        min_version: 1,
    },
    ProtocolSpec {
        name: "zwlr_foreign_toplevel_manager_v1",
        tier: Tier::Recommended,
        min_version: 1,
    },
    ProtocolSpec {
        name: "zwp_text_input_manager_v3",
        tier: Tier::Recommended,
        min_version: 1,
    },
    // --- optional extras ---
    ProtocolSpec {
        name: "wl_output",
        tier: Tier::Optional,
        min_version: 1,
    },
    ProtocolSpec {
        name: "zwp_idle_inhibit_manager_v1",
        tier: Tier::Optional,
        min_version: 1,
    },
];

/// Collapse raw registry events into one summary per interface.
///
/// Rules: group by interface name, keep the **highest** advertised version
/// (the version a client would actually bind), count instances, drop duplicate
/// registry ids, and sort by name so the JSON output is stable regardless of
/// compositor advertisement order.
pub fn dedupe_globals(globals: &[RawGlobal]) -> Vec<GlobalSummary> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut summaries: Vec<GlobalSummary> = Vec::new();
    for global in globals {
        if !seen_ids.insert(global.id) {
            continue; // duplicate registry id: ignore
        }
        match summaries.iter_mut().find(|s| s.name == global.interface) {
            Some(summary) => {
                summary.instances += 1;
                if global.version > summary.version {
                    summary.version = global.version;
                }
            }
            None => summaries.push(GlobalSummary {
                name: global.interface.clone(),
                version: global.version,
                instances: 1,
            }),
        }
    }
    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    summaries
}

/// Best-effort compositor family from distinctive registry interfaces.
///
/// `wl_compositor` has no name/version handshake that identifies the
/// implementation, so this stays a labeled heuristic (`family_basis` shows
/// the global that triggered the guess; `unknown` is a valid answer).
pub fn compositor_family(globals: &[GlobalSummary]) -> CompositorInfo {
    let find_version = |name: &str| globals.iter().find(|g| g.name == name).map(|g| g.version);
    let (family, basis) = match globals.iter().find(|g| g.name.starts_with("zwlr_")) {
        Some(g) => ("wlroots", Some(g.name.clone())),
        None => match globals.iter().find(|g| g.name.starts_with("org_kde_")) {
            Some(g) => ("kde", Some(g.name.clone())),
            None => match globals
                .iter()
                .find(|g| g.name.starts_with("weston_") || g.name.starts_with("zcosmic_"))
            {
                Some(g) if g.name.starts_with("weston_") => ("weston", Some(g.name.clone())),
                Some(g) => ("cosmic", Some(g.name.clone())),
                None => ("unknown", None),
            },
        },
    };
    CompositorInfo {
        family: family.to_string(),
        family_basis: basis,
        wl_compositor_version: find_version("wl_compositor"),
        xdg_wm_base_version: find_version("xdg_wm_base"),
    }
}

fn entry_for(spec: &ProtocolSpec, globals: &[GlobalSummary]) -> ProtocolEntry {
    match globals.iter().find(|g| g.name == spec.name) {
        Some(summary) => {
            let status = if summary.version >= spec.min_version {
                ProtocolStatus::Ok
            } else {
                ProtocolStatus::Outdated
            };
            ProtocolEntry {
                name: spec.name.to_string(),
                status,
                advertised_version: Some(summary.version),
                min_version: spec.min_version,
                instances: summary.instances,
            }
        }
        None => ProtocolEntry {
            name: spec.name.to_string(),
            status: ProtocolStatus::Missing,
            advertised_version: None,
            min_version: spec.min_version,
            instances: 0,
        },
    }
}

fn is_unusable(entry: &ProtocolEntry) -> bool {
    matches!(
        entry.status,
        ProtocolStatus::Missing | ProtocolStatus::Outdated
    )
}

/// Classify deduplicated globals into the tiers, tiers into a report.
///
/// `overall` is `fail` when a required protocol is missing/outdated, else
/// `degraded` when a recommended one is, else `pass`. Optional protocols never
/// change the verdict.
pub fn build_report(
    connection: crate::model::ConnectionInfo,
    globals: &[RawGlobal],
) -> ProbeReport {
    let summaries = dedupe_globals(globals);
    let mut required = Vec::new();
    let mut recommended = Vec::new();
    let mut optional = Vec::new();
    for spec in PROTOCOL_SPECS {
        let entry = entry_for(spec, &summaries);
        match spec.tier {
            Tier::Required => required.push(entry),
            Tier::Recommended => recommended.push(entry),
            Tier::Optional => optional.push(entry),
        }
    }

    let missing_required: Vec<String> = required
        .iter()
        .filter(|e| is_unusable(e))
        .map(|e| e.name.clone())
        .collect();
    let missing_recommended: Vec<String> = recommended
        .iter()
        .filter(|e| is_unusable(e))
        .map(|e| e.name.clone())
        .collect();

    let connected = connection.status == crate::model::ConnectionStatus::Connected;
    let overall = if !connected || !missing_required.is_empty() {
        OverallStatus::Fail
    } else if !missing_recommended.is_empty() {
        OverallStatus::Degraded
    } else {
        OverallStatus::Pass
    };

    ProbeReport {
        tool: TOOL_NAME.to_string(),
        schema_version: REPORT_SCHEMA_VERSION,
        connection,
        compositor: compositor_family(&summaries),
        globals: summaries,
        required,
        recommended,
        optional,
        missing_required,
        missing_recommended,
        overall,
    }
}
