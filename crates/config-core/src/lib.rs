//! config-core — headless configuration core (M0/M1 boundary).
//!
//! M0/M1 scope: pure data model over TOML with an integer `schema_version`,
//! pure `load`/`save` functions (same-dir temp file + fsync + atomic rename),
//! and a pure-function version migration framework. Explicit errors for
//! corrupt files, future schema versions and unknown fields.
//!
//! Out of scope (later milestones): config daemon, change notification / file
//! watching, revision re-read under write lock, conflict replay. This crate
//! must stay usable without any UI toolkit or async runtime.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Schema version written and understood by this build.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;
/// Oldest schema version this build can migrate from.
pub const MIN_SCHEMA_VERSION: i64 = 1;

/// Bounds used by [`Config::validate`]; documented so settings UI can clamp.
pub const ANIMATION_DURATION_MS_RANGE: (u32, u32) = (0, 2000);
pub const PANEL_HEIGHT_PX_RANGE: (u32, u32) = (8, 256);
pub const PANEL_MODULES_MAX: usize = 32;

/// Errors for loading, validating and saving configuration.
/// Every variant is diagnosable without logging raw user content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Filesystem failure (read/write/rename/fsync) with path context.
    Io { path: String, source: String },
    /// TOML syntax is corrupt.
    Parse { detail: String },
    /// Top-level integer `schema_version` key is absent.
    MissingSchemaVersion,
    /// `schema_version` is present but not a non-negative integer.
    InvalidSchemaVersion { value: String },
    /// File was written by a newer build; it must never be migrated down or
    /// overwritten by this process (read-only protection).
    FutureVersion { found: i64, supported: i64 },
    /// Version below [`MIN_SCHEMA_VERSION`] (e.g. 0 or negative).
    UnknownVersion { found: i64 },
    /// Structurally valid TOML but contains keys outside the schema.
    UnknownField { detail: String },
    /// Semantically invalid values (out-of-range numbers, empty strings).
    Invalid { detail: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "config io error at {path}: {source}")
            }
            ConfigError::Parse { detail } => write!(f, "config parse error: {detail}"),
            ConfigError::MissingSchemaVersion => {
                write!(f, "config missing required integer key `schema_version`")
            }
            ConfigError::InvalidSchemaVersion { value } => {
                write!(
                    f,
                    "config `schema_version` must be an integer, got {value:?}"
                )
            }
            ConfigError::FutureVersion { found, supported } => write!(
                f,
                "config schema_version {found} is newer than supported {supported}; \
                 file is treated read-only, upgrade the application instead of downgrading it"
            ),
            ConfigError::UnknownVersion { found } => write!(
                f,
                "config schema_version {found} is below the minimum {MIN_SCHEMA_VERSION}"
            ),
            ConfigError::UnknownField { detail } => write!(f, "config unknown field: {detail}"),
            ConfigError::Invalid { detail } => write!(f, "config invalid value: {detail}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Root configuration document. Sections cover launcher / panel /
/// notification / settings surfaces minimally but usefully.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: i64,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub animation: AnimationConfig,
    #[serde(default)]
    pub panel: PanelConfig,
    #[serde(default)]
    pub launcher: LauncherConfig,
    #[serde(default)]
    pub notification: NotificationConfig,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: ThemePreference::Auto,
            icon_theme: "hicolor".to_string(),
        }
    }
}

/// Look-and-feel shared by launcher, panel and notifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AppearanceConfig {
    /// Light/dark preference; `auto` defers to the portal/compositor.
    pub theme: ThemePreference,
    /// Icon theme name; empty means the default `hicolor`.
    pub icon_theme: String,
}

/// Motion settings; reduced-footprint devices may disable all animation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationConfig {
    pub enable_animations: bool,
    /// Upper bound for one animation, in milliseconds.
    pub duration_ms: u32,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enable_animations: true,
            duration_ms: 150,
        }
    }
}

/// Panel geometry and module order (rendered left-to-right / top-to-bottom).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelConfig {
    pub position: PanelPosition,
    pub height_px: u32,
    pub modules: Vec<String>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            position: PanelPosition::Bottom,
            height_px: 28,
            modules: vec!["clock".into()],
        }
    }
}

/// Launcher behaviour.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LauncherConfig {
    /// Search history switch: when false nothing about queries is persisted.
    pub remember_search_history: bool,
}

/// Notification behaviour and privacy surface.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NotificationConfig {
    /// Privacy: when false notification bodies are never stored or relayed to
    /// the popup; only the summary is displayed.
    pub privacy: NotificationPrivacy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationPrivacy {
    /// Whether popup surfaces may render notification bodies at all.
    pub show_body: bool,
}

impl Default for NotificationPrivacy {
    fn default() -> Self {
        Self { show_body: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelPosition {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

/// Pure semantic validation on top of serde's structural checks.
pub fn validate(cfg: &Config) -> Result<(), ConfigError> {
    let (lo, hi) = ANIMATION_DURATION_MS_RANGE;
    if cfg.animation.duration_ms < lo || cfg.animation.duration_ms > hi {
        return Err(ConfigError::Invalid {
            detail: format!(
                "animation.duration_ms {} outside {}..={hi}",
                cfg.animation.duration_ms, lo
            ),
        });
    }
    let (plo, phi) = PANEL_HEIGHT_PX_RANGE;
    if cfg.panel.height_px < plo || cfg.panel.height_px > phi {
        return Err(ConfigError::Invalid {
            detail: format!(
                "panel.height_px {} outside {}..={}",
                cfg.panel.height_px, plo, phi
            ),
        });
    }
    if cfg.panel.modules.len() > PANEL_MODULES_MAX {
        return Err(ConfigError::Invalid {
            detail: format!(
                "panel.modules count {} exceeds {PANEL_MODULES_MAX}",
                cfg.panel.modules.len()
            ),
        });
    }
    if cfg.appearance.icon_theme.is_empty() {
        return Err(ConfigError::Invalid {
            detail: "appearance.icon_theme must not be empty".into(),
        });
    }
    if cfg.panel.modules.iter().any(|m| m.trim().is_empty()) {
        return Err(ConfigError::Invalid {
            detail: "panel.modules must not contain empty names".into(),
        });
    }
    Ok(())
}

/// Classify a serde/toml message into an explicit error kind. Messages are
/// prefixed with TOML position information, so match on substrings.
pub(crate) fn classify_message(detail: String) -> ConfigError {
    if detail.contains("unknown field") {
        ConfigError::UnknownField { detail }
    } else {
        ConfigError::Parse { detail }
    }
}

pub mod io;
pub mod migrate;

#[cfg(test)]
mod tests;
