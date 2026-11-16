//! wayland-probe — independent Wayland capability detection (M0 baseline tool).
//!
//! Scope: open one client connection (`$WAYLAND_DISPLAY` / `$XDG_RUNTIME_DIR`,
//! or an absolute `$WAYLAND_DISPLAY` path, or the `WAYLAND_SOCKET` fd), do a
//! single `wl_display` roundtrip, collect the `wl_registry` globals and turn
//! them into a **stable JSON report** that classifies the protocols this
//! project needs into `required` / `recommended` / `optional`.
//!
//! This tool is strictly a **probe**: it binds nothing, creates no surface,
//! runs no event loop beyond the initial roundtrip, and a `pass` result does
//! NOT prove that Slint or smithay-client-toolkit will integrate successfully
//! (second-connection, frame delivery and IME behavior are separate M0 gates).
//!
//! All classification logic is pure functions over plain data
//! ([`model`], [`classify`]); only [`connection`] touches the environment or
//! the socket, so the report can be unit-tested without a compositor.

pub mod classify;
pub mod connection;
pub mod model;

#[cfg(test)]
mod json_tests;
#[cfg(test)]
mod tests;

pub use classify::{build_report, dedupe_globals, ProtocolSpec, Tier, PROTOCOL_SPECS};
pub use connection::{probe_environment, ConnectFailure, ConnectFailureKind, EnvironmentSnapshot};
pub use model::{
    CompositorInfo, ConnectionErrorInfo, ConnectionInfo, ConnectionStatus, GlobalSummary,
    OverallStatus, ProbeReport, ProtocolEntry, ProtocolStatus, RawGlobal,
};
