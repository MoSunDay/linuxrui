//! Line-oriented Desktop Entry parser (std only, no GTK).
//!
//! Handles `[Desktop Entry]`, `[Desktop Action <id>]` (unknown groups are
//! ignored per spec), full-line `#` comments, `key=value` with optional locale
//! suffix (`Name[zh_CN]`), booleans, plain strings and `;`-separated string
//! lists. First occurrence wins for duplicate keys. Invalid actions are
//! dropped with diagnostics while the main entry survives; only structural
//! breakage (no main group, missing `Type`/`Name`, application without `Exec`)
//! is fatal.

use crate::localized::{split_string_list, LocalizedString, LocalizedStringList};
use crate::{DesktopAction, DesktopEntry, Diagnostic, DiagnosticKind, EntryType, ParseError};

pub fn parse_desktop_entry(text: &str) -> Result<DesktopEntry, ParseError> {
    let mut entry = DesktopEntry::default();
    let mut seen_main = false;
    let mut actions: Vec<DesktopAction> = Vec::new();
    let mut listed_ids: Vec<String> = Vec::new();
    let mut group: Group = Group::BeforeFirst;
    let mut seen_keys: Vec<String> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let Some(name) = header.strip_suffix(']') else {
                push_diag(&mut entry, line_no, DiagnosticKind::IgnoredGroup, line);
                continue;
            };
            if name == "Desktop Entry" {
                seen_main = true;
                seen_keys.clear();
                group = Group::Main;
            } else if let Some(action_id) = name.strip_prefix("Desktop Action ") {
                if is_action_id(action_id) {
                    actions.push(DesktopAction {
                        id: action_id.to_string(),
                        ..Default::default()
                    });
                    seen_keys.clear();
                    group = Group::Action(actions.len() - 1);
                } else {
                    push_diag(&mut entry, line_no, DiagnosticKind::DroppedAction, name);
                    group = Group::Ignored;
                }
            } else {
                push_diag(&mut entry, line_no, DiagnosticKind::IgnoredGroup, name);
                group = Group::Ignored;
            }
            continue;
        }
        let Some((raw_key, value)) = line.split_once('=') else {
            push_diag(&mut entry, line_no, DiagnosticKind::InvalidKey, line);
            continue;
        };
        let key = parse_key(raw_key);
        let Some(KeyRef { name, locale }) = key else {
            push_diag(&mut entry, line_no, DiagnosticKind::InvalidKey, raw_key);
            continue;
        };
        let unique = format!("{group:?}/{name}/{}", locale.as_deref().unwrap_or_default());
        if seen_keys.contains(&unique) {
            push_diag(&mut entry, line_no, DiagnosticKind::DuplicateKey, name);
            continue;
        }
        seen_keys.push(unique);
        let value = unescape_value(value);
        match group {
            Group::BeforeFirst => {
                push_diag(&mut entry, line_no, DiagnosticKind::KeyBeforeGroup, name);
            }
            Group::Ignored => {}
            Group::Main if name == "Actions" => listed_ids = split_string_list(&value),
            Group::Main => apply_main_key(&mut entry, name, locale, &value),
            Group::Action(i) => apply_action_key(&mut actions[i], name, locale, &value),
        }
    }
    if !seen_main {
        return Err(ParseError::MissingMainGroup);
    }
    if entry.entry_type.is_none() {
        return Err(ParseError::MissingRequiredKey { key: "Type" });
    }
    if entry.name.is_empty() {
        return Err(ParseError::MissingRequiredKey { key: "Name" });
    }
    if entry
        .entry_type
        .as_ref()
        .is_some_and(EntryType::is_application)
        && entry.exec.is_none()
        && !entry.d_bus_activatable
    {
        return Err(ParseError::ApplicationWithoutExec);
    }
    // Keep only actions that were both listed in Actions= and carry the
    // required keys; report each drop so callers can diagnose bad files.
    let mut kept = Vec::new();
    for action in actions {
        if !listed_ids.contains(&action.id) {
            entry.diagnostics.push(Diagnostic {
                line: 0,
                kind: DiagnosticKind::MissingActionGroup,
                detail: format!(
                    "group [Desktop Action {}] is not referenced by Actions=",
                    action.id
                ),
            });
            continue;
        }
        if action.name.is_empty() || action.exec.is_none() {
            entry.diagnostics.push(Diagnostic {
                line: 0,
                kind: DiagnosticKind::DroppedAction,
                detail: format!("action {} lacks required Name/Exec", action.id),
            });
            continue;
        }
        kept.push(action);
    }
    for id in &listed_ids {
        if !kept.iter().any(|a: &DesktopAction| &a.id == id) {
            entry.diagnostics.push(Diagnostic {
                line: 0,
                kind: DiagnosticKind::MissingActionGroup,
                detail: format!("Actions= lists {id} without a matching group"),
            });
        }
    }
    entry.actions = kept;
    Ok(entry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    BeforeFirst,
    Main,
    Action(usize),
    Ignored,
}

struct KeyRef<'a> {
    name: &'a str,
    locale: Option<String>,
}

fn push_diag(entry: &mut DesktopEntry, line: usize, kind: DiagnosticKind, detail: &str) {
    entry.diagnostics.push(Diagnostic {
        line,
        kind,
        detail: detail.to_string(),
    });
}

/// `Name` / `Name[zh_CN]`; key grammar `[A-Za-z0-9-]+`, locale suffix
/// `[A-Za-z0-9_.@+-]+`.
fn parse_key(raw: &str) -> Option<KeyRef<'_>> {
    let (name, locale) = match raw.find('[') {
        None => (raw.trim(), None),
        Some(open) => {
            let close = raw.rfind(']')?;
            if close != raw.len() - 1 || open + 1 >= close {
                return None;
            }
            let locale = &raw[open + 1..close];
            if !is_locale(locale) {
                return None;
            }
            (&raw[..open], Some(locale.to_string()))
        }
    };
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(KeyRef { name, locale })
}

fn is_locale(locale: &str) -> bool {
    !locale.is_empty()
        && locale
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '+' | '-'))
}

fn is_action_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The spec allows `\s`, `\n`, `\t` and `\\` escapes in values.
fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                // Unknown escape: keep the backslash literally.
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn apply_main_key(entry: &mut DesktopEntry, name: &str, locale: Option<String>, value: &str) {
    let invalid_bool = |entry: &mut DesktopEntry| {
        entry.diagnostics.push(Diagnostic {
            line: 0,
            kind: DiagnosticKind::InvalidBoolean,
            detail: format!("{name}={value} is not true/false"),
        });
    };
    match name {
        "Type" => entry.entry_type = Some(EntryType::from_key(value)),
        "Version" => entry.version = Some(value.to_string()),
        "Name" => set_localized(&mut entry.name, locale, value),
        "GenericName" => set_localized(&mut entry.generic_name, locale, value),
        "Comment" => set_localized(&mut entry.comment, locale, value),
        "Keywords" => set_localized_list(&mut entry.keywords, locale, split_string_list(value)),
        "Exec" => entry.exec = Some(value.to_string()),
        "TryExec" => entry.try_exec = Some(value.to_string()),
        "Icon" => entry.icon = Some(value.to_string()),
        "Path" => entry.path = Some(value.to_string()),
        "URL" => entry.url = Some(value.to_string()),
        "Terminal" => match parse_bool(value) {
            Some(v) => entry.terminal = v,
            None => invalid_bool(entry),
        },
        "DBusActivatable" => match parse_bool(value) {
            Some(v) => entry.d_bus_activatable = v,
            None => invalid_bool(entry),
        },
        "Hidden" => match parse_bool(value) {
            Some(v) => entry.hidden = v,
            None => invalid_bool(entry),
        },
        "NoDisplay" => match parse_bool(value) {
            Some(v) => entry.no_display = v,
            None => invalid_bool(entry),
        },
        "OnlyShowIn" => entry.only_show_in = split_string_list(value),
        "NotShowIn" => entry.not_show_in = split_string_list(value),
        // Unknown keys are ignored per spec, without diagnostics.
        _ => {}
    }
}

fn apply_action_key(action: &mut DesktopAction, name: &str, locale: Option<String>, value: &str) {
    match name {
        "Name" => set_localized(&mut action.name, locale, value),
        "Icon" => action.icon = Some(value.to_string()),
        "Exec" => action.exec = Some(value.to_string()),
        _ => {}
    }
}

fn set_localized(slot: &mut LocalizedString, locale: Option<String>, value: &str) {
    match locale {
        Some(tag) => slot.set_locale(tag, value.to_string()),
        None => slot.set_default(value.to_string()),
    }
}

fn set_localized_list(slot: &mut LocalizedStringList, locale: Option<String>, values: Vec<String>) {
    match locale {
        Some(tag) => slot.set_locale(tag, values),
        None => slot.set_default(values),
    }
}
