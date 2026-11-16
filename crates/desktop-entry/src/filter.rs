//! Pure visibility filtering: desktop-ID priority with `Hidden` masking,
//! `NoDisplay`, `OnlyShowIn`/`NotShowIn` against `$XDG_CURRENT_DESKTOP`, and
//! `TryExec` PATH lookup. Scanning/watching is a later milestone; these
//! functions only operate on already-parsed [`LocatedEntry`] values.

use crate::{DesktopEntry, EntryType};
use std::path::{Path, PathBuf};

/// One parsed file with its desktop ID, source path and search priority.
/// Lower `priority` wins (0 = first `$XDG_DATA_DIRS/applications` entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedEntry {
    pub entry: DesktopEntry,
    /// Desktop ID, e.g. `org.example.App.desktop`.
    pub desktop_id: String,
    pub source_path: PathBuf,
    pub priority: u8,
}

/// Inputs to visibility filtering. `try_exec_available` is a function so tests
/// and callers can supply their own resolution (container, sandbox, ...).
pub struct FilterEnv<F>
where
    F: Fn(&str) -> bool,
{
    /// Entries of `$XDG_CURRENT_DESKTOP`, split on `:` (see
    /// [`parse_current_desktops`]). Empty means the variable is unset.
    pub current_desktops: Vec<String>,
    pub try_exec_available: F,
}

/// Split `$XDG_CURRENT_DESKTOP` on `:` and drop empty components.
pub fn parse_current_desktops(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Resolve duplicates by desktop ID: the lowest priority entry wins, and a
/// winning `Hidden=true` masks the whole ID (spec: Hidden means "deleted").
/// Masked IDs are excluded from the result entirely.
pub fn resolve_desktop_ids(items: &[LocatedEntry]) -> Vec<&LocatedEntry> {
    let mut winners: Vec<&LocatedEntry> = Vec::new();
    for item in items {
        match winners.iter_mut().find(|w| w.desktop_id == item.desktop_id) {
            Some(current) => {
                if item.priority < current.priority {
                    *current = item;
                }
            }
            None => winners.push(item),
        }
    }
    winners.into_iter().filter(|w| !w.entry.hidden).collect()
}

/// Pure visibility rule for one resolved entry (dedupe already applied).
///
/// - Only `Type=Application` items are launchable from listings.
/// - `NoDisplay` removes the item from listings (still launchable by ID).
/// - `NotShowIn` takes precedence over `OnlyShowIn` when both are set.
/// - `OnlyShowIn`, when non-empty, requires a match in `current_desktops`;
///   with the variable unset no restricted item matches.
/// - `TryExec` must resolve to an executable file.
pub fn is_visible(
    entry: &DesktopEntry,
    current_desktops: &[String],
    try_exec_available: &dyn Fn(&str) -> bool,
) -> bool {
    if !entry
        .entry_type
        .as_ref()
        .is_some_and(EntryType::is_application)
    {
        return false;
    }
    if entry.no_display {
        return false;
    }
    if let Some(try_exec) = &entry.try_exec {
        if !try_exec_available(try_exec) {
            return false;
        }
    }
    if entry
        .not_show_in
        .iter()
        .any(|d| current_desktops.contains(d))
    {
        return false;
    }
    if !entry.only_show_in.is_empty()
        && !entry
            .only_show_in
            .iter()
            .any(|d| current_desktops.contains(d))
    {
        return false;
    }
    true
}

/// Combined pipeline: dedupe by desktop ID, drop `Hidden`-masked IDs, then
/// apply [`is_visible`].
pub fn visible_entries<'a, F>(
    items: &'a [LocatedEntry],
    env: &FilterEnv<F>,
) -> Vec<&'a LocatedEntry>
where
    F: Fn(&str) -> bool,
{
    resolve_desktop_ids(items)
        .into_iter()
        .filter(|item| is_visible(&item.entry, &env.current_desktops, &env.try_exec_available))
        .collect()
}

/// Resolve `TryExec` against a PATH-style variable. A value containing a path
/// separator is used as-is (absolute or relative); a bare name is searched in
/// each PATH component, requiring a regular executable file. No PATH means no
/// match.
pub fn find_in_path(name: &str, path: Option<&str>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    if name.is_empty() {
        return None;
    }
    let is_exec_file = |p: &Path| -> bool {
        match std::fs::metadata(p) {
            Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
            Err(_) => false,
        }
    };
    if name.contains('/') {
        let p = PathBuf::from(name);
        return if is_exec_file(&p) { Some(p) } else { None };
    }
    for dir in path.unwrap_or_default().split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(name);
        if is_exec_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}
