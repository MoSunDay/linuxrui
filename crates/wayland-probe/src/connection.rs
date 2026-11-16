//! Live connection: env snapshot, `connect_to_env`, one registry roundtrip.
//!
//! Uses the `wayland-client` 0.31 [`wayland_client::globals`] helper:
//! `registry_queue_init` performs the initial roundtrip and keeps the global
//! list in `GlobalListContents`; the required `Dispatch<WlRegistry, _>` impl
//! below intentionally does nothing because a probe only snapshots the initial
//! advertisement. Failures are returned as data, never panicked on.

use std::env;

use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::wl_registry,
    Connection, Dispatch, QueueHandle,
};

use crate::model::{ConnectionErrorInfo, ConnectionInfo, ConnectionStatus, RawGlobal};

/// Machine-readable reason a connection attempt failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectFailureKind {
    /// `$WAYLAND_DISPLAY` is not set (and no `WAYLAND_SOCKET` fd).
    NoWaylandDisplayEnv,
    /// `$WAYLAND_DISPLAY` is relative but `$XDG_RUNTIME_DIR` is not set/absolute.
    NoXdgRuntimeDirEnv,
    /// Socket exists in principle but `connect(2)` failed.
    ConnectFailed,
    /// `WAYLAND_SOCKET` was set but did not hold a usable fd number.
    InvalidFd,
    /// System libwayland could not be loaded (system backend builds only).
    NoWaylandLib,
    /// Connected, but the registry roundtrip errored.
    RegistryError,
}

impl ConnectFailureKind {
    /// Stable snake_case identifier used in the JSON `connection.error.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectFailureKind::NoWaylandDisplayEnv => "no_wayland_display_env",
            ConnectFailureKind::NoXdgRuntimeDirEnv => "no_xdg_runtime_dir_env",
            ConnectFailureKind::ConnectFailed => "connect_failed",
            ConnectFailureKind::InvalidFd => "invalid_fd",
            ConnectFailureKind::NoWaylandLib => "no_wayland_lib",
            ConnectFailureKind::RegistryError => "registry_error",
        }
    }
}

/// Connection failure as reportable data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectFailure {
    /// Failure class.
    pub kind: ConnectFailureKind,
    /// Human-readable explanation (no env var values, only their names).
    pub message: String,
}

impl ConnectFailure {
    /// Build from a kind plus context; message is derived per kind.
    pub fn new(kind: ConnectFailureKind, detail: &str) -> Self {
        let message = match kind {
            ConnectFailureKind::NoWaylandDisplayEnv => {
                "WAYLAND_DISPLAY is not set and no usable WAYLAND_SOCKET fd was provided"
                    .to_string()
            }
            ConnectFailureKind::NoXdgRuntimeDirEnv => {
                "WAYLAND_DISPLAY is relative but XDG_RUNTIME_DIR is missing or not absolute"
                    .to_string()
            }
            ConnectFailureKind::ConnectFailed => {
                format!("could not connect to the compositor socket: {detail}")
            }
            ConnectFailureKind::InvalidFd => {
                "WAYLAND_SOCKET was set but did not contain a usable fd".to_string()
            }
            ConnectFailureKind::NoWaylandLib => {
                "the system wayland library could not be loaded".to_string()
            }
            ConnectFailureKind::RegistryError => {
                format!("connected, but the registry roundtrip failed: {detail}")
            }
        };
        Self { kind, message }
    }

    /// Convert into the report's `connection` object (`unavailable` form).
    pub fn into_connection_info(self, snapshot: &EnvironmentSnapshot) -> ConnectionInfo {
        ConnectionInfo {
            status: ConnectionStatus::Unavailable,
            wayland_display: snapshot.wayland_display.clone(),
            xdg_runtime_dir: snapshot.xdg_runtime_dir.clone(),
            error: Some(ConnectionErrorInfo {
                kind: self.kind.as_str().to_string(),
                message: self.message,
            }),
        }
    }
}

/// Values of the relevant environment variables, read once before connecting.
///
/// `connect_to_env` consumes `WAYLAND_SOCKET`, so the snapshot is taken first;
/// only variable *presence* is reported (values of `WAYLAND_DISPLAY` /
/// `XDG_RUNTIME_DIR` are socket names/paths, not secrets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentSnapshot {
    /// `$WAYLAND_DISPLAY` value if set.
    pub wayland_display: Option<String>,
    /// `$XDG_RUNTIME_DIR` value if set.
    pub xdg_runtime_dir: Option<String>,
    /// Whether `$WAYLAND_SOCKET` was set (the fd number itself is not kept).
    pub wayland_socket_set: bool,
}

impl EnvironmentSnapshot {
    /// Read the environment once (pure with respect to the process env).
    ///
    /// Uses `var_os` so *presence* matches `Connection::connect_to_env`
    /// exactly; non-UTF-8 values are lossily converted for the JSON report.
    pub fn capture() -> Self {
        let lossy = |key: &str| env::var_os(key).map(|v| v.to_string_lossy().into_owned());
        Self {
            wayland_display: lossy("WAYLAND_DISPLAY"),
            xdg_runtime_dir: lossy("XDG_RUNTIME_DIR"),
            wayland_socket_set: env::var_os("WAYLAND_SOCKET").is_some(),
        }
    }
}

/// State for the `wayland-client` registry helper.
///
/// A probe takes one snapshot, so dynamic add/remove events after the initial
/// roundtrip are deliberately ignored (the helper keeps its own list).
#[derive(Debug, Default)]
pub struct ProbeState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ProbeState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Initial globals are collected by `registry_queue_init`; nothing to do.
    }
}

/// Outcome of [`probe_environment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Connected; the initial registry advertisement as raw globals.
    Connected(Vec<RawGlobal>),
    /// Never connected; see the failure data.
    Failed(ConnectFailure),
}

/// Connect via the environment and snapshot the registry in one roundtrip.
///
/// Env diagnostics run first so a missing variable produces a precise
/// `kind` instead of `wayland-client`'s generic `NoCompositor`.
pub fn probe_environment() -> ProbeOutcome {
    probe_with_snapshot(&EnvironmentSnapshot::capture())
}

/// Probe against one already-captured environment snapshot.
///
/// The same snapshot feeds validation, the connection attempt, and the
/// caller's report, so one probe never mixes two environment samples
/// (e.g. when the process environment changes mid-run).
pub fn probe_with_snapshot(snapshot: &EnvironmentSnapshot) -> ProbeOutcome {
    match connect_and_collect(snapshot) {
        Ok(globals) => ProbeOutcome::Connected(globals),
        Err(failure) => ProbeOutcome::Failed(failure),
    }
}

/// Connection info for a successful run (errors come via [`ConnectFailure`]).
pub fn connected_info(snapshot: &EnvironmentSnapshot) -> ConnectionInfo {
    ConnectionInfo {
        status: ConnectionStatus::Connected,
        wayland_display: snapshot.wayland_display.clone(),
        xdg_runtime_dir: snapshot.xdg_runtime_dir.clone(),
        error: None,
    }
}

fn connect_and_collect(snapshot: &EnvironmentSnapshot) -> Result<Vec<RawGlobal>, ConnectFailure> {
    // Precise diagnostics for the two "unset env" cases wayland-client
    // folds into NoCompositor. An absolute WAYLAND_DISPLAY needs no runtime dir.
    if snapshot.wayland_display.is_none() && !snapshot.wayland_socket_set {
        return Err(ConnectFailure::new(
            ConnectFailureKind::NoWaylandDisplayEnv,
            "",
        ));
    }
    let display_is_absolute = snapshot
        .wayland_display
        .as_deref()
        .map(|d| d.starts_with('/'))
        .unwrap_or(false);
    // WAYLAND_SOCKET (an inherited fd) takes priority in connect_to_env, so
    // the XDG_RUNTIME_DIR requirement only applies to socket-name lookups.
    if !snapshot.wayland_socket_set
        && snapshot.wayland_display.is_some()
        && !display_is_absolute
        && snapshot
            .xdg_runtime_dir
            .as_deref()
            .map(|dir| !dir.starts_with('/'))
            .unwrap_or(true)
    {
        return Err(ConnectFailure::new(
            ConnectFailureKind::NoXdgRuntimeDirEnv,
            "",
        ));
    }

    let conn = Connection::connect_to_env().map_err(|err| {
        let kind = match err {
            wayland_client::ConnectError::InvalidFd => ConnectFailureKind::InvalidFd,
            wayland_client::ConnectError::NoWaylandLib => ConnectFailureKind::NoWaylandLib,
            wayland_client::ConnectError::NoCompositor => ConnectFailureKind::ConnectFailed,
        };
        ConnectFailure::new(kind, &err.to_string())
    })?;

    collect_globals(&conn)
}

/// Run the registry roundtrip on an already-established connection.
///
/// Split from [`probe_environment`] so tests can drive a socketpair-backed
/// fake compositor through [`Connection::from_socket`] without touching the
/// process environment.
pub fn collect_globals(conn: &Connection) -> Result<Vec<RawGlobal>, ConnectFailure> {
    let (global_list, _queue) = registry_queue_init::<ProbeState>(conn)
        .map_err(|err| ConnectFailure::new(ConnectFailureKind::RegistryError, &err.to_string()))?;
    let globals = global_list
        .contents()
        .clone_list()
        .into_iter()
        .map(|g| RawGlobal {
            id: g.name,
            interface: g.interface,
            version: g.version,
        })
        .collect();
    Ok(globals)
}
