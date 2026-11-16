//! Plain data model for the probe report.
//!
//! Everything here is a data `struct`/`enum` with `serde` derives; behavior
//! lives in [`crate::classify`] as free functions. The JSON field names are
//! part of the tool's contract (`schema_version` guards future changes).

use serde::{Deserialize, Serialize};

/// JSON schema version of the report; bump on any breaking field change.
pub const REPORT_SCHEMA_VERSION: u32 = 1;
/// Tool identifier written into every report.
pub const TOOL_NAME: &str = "wayland-probe";

/// One raw `wl_registry.global` event as received from the compositor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawGlobal {
    /// Registry-global numeric id (unique per connection, not stable across sessions).
    pub id: u32,
    /// Interface name, e.g. `wl_compositor`.
    pub interface: String,
    /// Advertised (maximum bindable) version.
    pub version: u32,
}

/// A global interface deduplicated across instances: best version + count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalSummary {
    /// Interface name, e.g. `wl_compositor`.
    pub name: String,
    /// Highest advertised version among all instances.
    pub version: u32,
    /// How many instances of this interface the registry advertised.
    pub instances: u32,
}

/// Connection outcome for the `connection` JSON object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    /// Roundtrip completed and globals were collected.
    Connected,
    /// Could not connect / roundtrip failed; see `error`.
    Unavailable,
}

/// Diagnosable connection failure without leaking environment contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionErrorInfo {
    /// Machine-readable failure class (see [`crate::ConnectFailureKind`]).
    pub kind: String,
    /// Human-readable explanation (no secrets; env var values are not included).
    pub message: String,
}

/// The `connection` object of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// `connected` or `unavailable`.
    pub status: ConnectionStatus,
    /// Value of `$WAYLAND_DISPLAY` if set (null otherwise).
    #[serde(rename = "wayland_display")]
    pub wayland_display: Option<String>,
    /// Value of `$XDG_RUNTIME_DIR` if set (null otherwise).
    #[serde(rename = "xdg_runtime_dir")]
    pub xdg_runtime_dir: Option<String>,
    /// Present only when `status` is `unavailable`.
    pub error: Option<ConnectionErrorInfo>,
}

/// Heuristic compositor identification from registry globals only.
///
/// There is no `wl_compositor` "name" field; this is a guess derived from
/// distinctive interfaces and must be treated as advisory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositorInfo {
    /// `wlroots` (sway/labwc/…), `kde`, `weston`, `cosmic` or `unknown`.
    pub family: String,
    /// The global that triggered the family guess (null when `unknown`).
    pub family_basis: Option<String>,
    /// Advertised `wl_compositor` version, null when absent.
    pub wl_compositor_version: Option<u32>,
    /// Advertised `xdg_wm_base` version, null when absent.
    pub xdg_wm_base_version: Option<u32>,
}

/// Per-protocol support verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolStatus {
    /// Present at or above the required minimum version.
    Ok,
    /// Present but older than the accepted minimum.
    Outdated,
    /// Not advertised at all.
    Missing,
}

/// One entry of the `required` / `recommended` / `optional` arrays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolEntry {
    /// Interface name, e.g. `zwlr_layer_shell_v1`.
    pub name: String,
    /// `ok`, `outdated` or `missing`.
    pub status: ProtocolStatus,
    /// Highest advertised version, null when missing.
    pub advertised_version: Option<u32>,
    /// Minimum version this project accepts.
    pub min_version: u32,
    /// Number of advertised instances (0 when missing).
    pub instances: u32,
}

/// Whole-report verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallStatus {
    /// All required and recommended protocols are `ok`.
    Pass,
    /// Required ok, but some recommended missing/outdated.
    Degraded,
    /// Connection failed, or a required protocol missing/outdated.
    Fail,
}

/// The complete probe report; serializes to the tool's stable JSON output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeReport {
    /// Always `"wayland-probe"`.
    pub tool: String,
    /// See [`REPORT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Connection environment + outcome.
    pub connection: ConnectionInfo,
    /// Heuristic compositor identification.
    pub compositor: CompositorInfo,
    /// All advertised interfaces, deduplicated, sorted by name.
    pub globals: Vec<GlobalSummary>,
    /// Baseline protocols the desktop components cannot work without.
    pub required: Vec<ProtocolEntry>,
    /// Protocols enabling full UX; absent ones degrade, not block.
    pub recommended: Vec<ProtocolEntry>,
    /// Nice-to-have protocols; reported for visibility only.
    pub optional: Vec<ProtocolEntry>,
    /// Names of required protocols that are missing or outdated.
    pub missing_required: Vec<String>,
    /// Names of recommended protocols that are missing or outdated.
    pub missing_recommended: Vec<String>,
    /// `pass`, `degraded` or `fail`.
    pub overall: OverallStatus,
}

/// Serialize a report to pretty JSON; the report is always valid JSON.
pub fn to_pretty_json(report: &ProbeReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| {
        // ProbeReport is a plain tree of serde types; serialization cannot
        // fail in practice. Fall back to a minimal valid document anyway.
        "{\"tool\":\"wayland-probe\",\"schema_version\":1,\"overall\":\"fail\"}".to_string()
    })
}

/// Serialize a report to compact single-line JSON.
pub fn to_compact_json(report: &ProbeReport) -> String {
    serde_json::to_string(report).unwrap_or_else(|_| {
        "{\"tool\":\"wayland-probe\",\"schema_version\":1,\"overall\":\"fail\"}".to_string()
    })
}
