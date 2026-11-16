//! desktop-entry — headless Desktop Entry parsing core (M0/M1 boundary).//!
//! M0/M1 scope: a std-only parser for `[Desktop Entry]` and
//! `[Desktop Action ...]` groups (comments, locale keys like `Name[zh_CN]`,
//! booleans, strings, string lists), the entry data model, locale fallback,
//! and pure filtering (desktop-ID priority with `Hidden` masking, `NoDisplay`,
//! `OnlyShowIn`/`NotShowIn` against `$XDG_CURRENT_DESKTOP`, `TryExec` lookup).
//!
//! Out of scope: icon lookup/rendering, Menu XML, .directory files as a
//! separate model, directory scanning/watching (later milestones add a bounded
//! scanner on top of this parser).

mod filter;
mod localized;
mod parse;

#[cfg(test)]
mod tests;

pub use filter::{
    find_in_path, is_visible, parse_current_desktops, resolve_desktop_ids, visible_entries,
    FilterEnv, LocatedEntry,
};
pub use localized::{LocalizedString, LocalizedStringList};
pub use parse::parse_desktop_entry;

/// Entry `Type` values the spec defines; anything else is preserved verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryType {
    Application,
    Link,
    Directory,
    Other(String),
}

impl EntryType {
    pub fn from_key(value: &str) -> EntryType {
        match value {
            "Application" => EntryType::Application,
            "Link" => EntryType::Link,
            "Directory" => EntryType::Directory,
            other => EntryType::Other(other.to_string()),
        }
    }

    /// Launcher listings only ever show `Type=Application` items.
    pub fn is_application(&self) -> bool {
        matches!(self, EntryType::Application)
    }
}

/// A parsed desktop entry. Fields follow the Desktop Entry Specification; the
/// value of `Actions` is stored parsed into [`DesktopAction`] records so the
/// invalid ones can be dropped with a diagnostic while keeping the main entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopEntry {
    pub entry_type: Option<EntryType>,
    /// Version key (`string`, e.g. "1.5"); informational only.
    pub version: Option<String>,
    pub name: LocalizedString,
    pub generic_name: LocalizedString,
    pub comment: LocalizedString,
    pub keywords: LocalizedStringList,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub icon: Option<String>,
    pub path: Option<String>,
    /// Present for `Type=Link`; kept for completeness.
    pub url: Option<String>,
    pub terminal: bool,
    pub d_bus_activatable: bool,
    pub hidden: bool,
    pub no_display: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub actions: Vec<DesktopAction>,
    /// Non-fatal problems collected while parsing (dropped actions, invalid
    /// booleans, duplicate keys, ...). Fatal problems abort with [`ParseError`].
    pub diagnostics: Vec<Diagnostic>,
}

/// One `[Desktop Action <id>]` group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopAction {
    pub id: String,
    pub name: LocalizedString,
    pub icon: Option<String>,
    pub exec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub kind: DiagnosticKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// A group header that is not `[Desktop Entry]` / `[Desktop Action ...]`.
    IgnoredGroup,
    /// Key name failed the spec's grammar.
    InvalidKey,
    /// Boolean key with a value other than `true`/`false` (treated as false).
    InvalidBoolean,
    /// Same key seen twice; first value wins per the spec.
    DuplicateKey,
    /// `Actions=` referenced an id without a matching group.
    MissingActionGroup,
    /// Action group dropped because its id or required keys are invalid.
    DroppedAction,
    /// Keys encountered before the first group header.
    KeyBeforeGroup,
}

/// Fatal parse failures: the document is not a usable desktop entry at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The file never opened a `[Desktop Entry]` group.
    MissingMainGroup,
    /// Required key missing in the main group (`Type` or `Name`).
    MissingRequiredKey { key: &'static str },
    /// `Type=Application` requires a non-empty `Exec` or `DBusActivatable=true`.
    ApplicationWithoutExec,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingMainGroup => write!(f, "missing [Desktop Entry] group"),
            ParseError::MissingRequiredKey { key } => write!(f, "missing required key {key}"),
            ParseError::ApplicationWithoutExec => {
                write!(f, "Type=Application without Exec or DBusActivatable=true")
            }
        }
    }
}

impl std::error::Error for ParseError {}
