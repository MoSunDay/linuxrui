//! launch-core — headless launch planning (M0/M1 boundary).
//!
//! M0/M1 scope: Desktop Entry `Exec` tokenization (double quoting, backslash
//! escaping — never a shell), field-code expansion (`%f %u %F %U %i %c %k %%`,
//! deprecated codes removed, unknown codes rejected), and producing an argv
//! plan plus activation-token metadata. `Terminal=true` currently yields the
//! explicit [`LaunchPlanError::UnsupportedTerminal`] — no terminal adapter is
//! invented here.
//!
//! Out of scope: spawning processes, D-Bus activation, zombie reaping and the
//! foot adapter (later milestones build the runtime launcher on this plan).

mod exec;
mod expand;

pub use exec::{tokenize, ExecToken, ExecTokenizeError, Piece};

use std::fmt;
use std::path::PathBuf;

/// One file/URL argument passed to the launched application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchFile {
    /// Local filesystem path.
    Path(String),
    /// URI (e.g. `file:///...` or `https://...`).
    Uri(String),
}

impl LaunchFile {
    pub fn as_str(&self) -> &str {
        match self {
            LaunchFile::Path(s) | LaunchFile::Uri(s) => s,
        }
    }
}

/// Activation token handed to the child for startup notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Wayland `xdg-activation-v1` token -> `XDG_ACTIVATION_TOKEN`.
    XdgActivationToken,
    /// Legacy DESKTOP_STARTUP_ID / `--startup-id` style -> `DESKTOP_STARTUP_ID`.
    DesktopStartupId,
}

impl TokenKind {
    pub fn env_var(self) -> &'static str {
        match self {
            TokenKind::XdgActivationToken => "XDG_ACTIVATION_TOKEN",
            TokenKind::DesktopStartupId => "DESKTOP_STARTUP_ID",
        }
    }
}

/// Everything needed to expand one Exec line into a plan.
#[derive(Debug, Clone, Default)]
pub struct LaunchRequest<'a> {
    /// Raw `Exec` value (already unescaped by desktop-entry parsing).
    pub exec: &'a str,
    /// `Terminal` key of the desktop entry.
    pub terminal: bool,
    /// Files/URLs for `%f %u %F %U`; empty means "no file".
    pub files: Vec<LaunchFile>,
    /// `Icon` key for `%i`.
    pub icon: Option<&'a str>,
    /// Localized display name for `%c` (resolved by the caller, e.g. zh_CN).
    pub display_name: Option<&'a str>,
    /// Location of the desktop file for `%k`; `None` removes the argument.
    pub desktop_file_path: Option<&'a str>,
    /// Activation token metadata forwarded to the child environment.
    pub activation: Option<(&'a str, TokenKind)>,
    /// `Path` key; the future spawner must chdir here successfully.
    pub working_directory: Option<&'a std::path::Path>,
}

/// Resulting process plan: argv created directly (execve), never a shell.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LaunchPlan {
    pub argv: Vec<String>,
    pub working_directory: Option<PathBuf>,
    /// Token to export in the child environment (see [`TokenKind::env_var`]).
    pub activation_env: Option<(String, &'static str)>,
}

/// Diagnosable planning failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPlanError {
    /// Exec line has no tokens.
    EmptyExec,
    /// Unterminated quote or dangling escape.
    Tokenize { detail: String },
    /// A `%x` that is neither supported nor deprecated (rejected per spec).
    UnknownFieldCode { code: String },
    /// `Terminal=true`: no terminal adapter exists in this milestone; do not
    /// fabricate one. The error names the entry so callers can surface it.
    UnsupportedTerminal { entry_hint: String },
}

impl fmt::Display for LaunchPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchPlanError::EmptyExec => write!(f, "Exec line produced no arguments"),
            LaunchPlanError::Tokenize { detail } => write!(f, "Exec tokenization failed: {detail}"),
            LaunchPlanError::UnknownFieldCode { code } => {
                write!(f, "Exec contains unknown field code {code:?}")
            }
            LaunchPlanError::UnsupportedTerminal { entry_hint } => write!(
                f,
                "Terminal=true is not supported yet (no terminal adapter in M1): {entry_hint}"
            ),
        }
    }
}

impl std::error::Error for LaunchPlanError {}

pub use expand::plan_launch;

#[cfg(test)]
mod tests;
