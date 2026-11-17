//! Pure application-index logic for the M0 shell UI.
//!
//! Scans XDG-style `applications` roots for `*.desktop` files, reuses
//! `desktop-entry` for parsing and visibility filtering, and `launch-core`
//! for a launch-plan preview (`Exec` field-code expansion). The functions
//! here only read the given directories; the UI layer keeps the single
//! event-loop owner and stays testable offline.

use std::fs;
use std::path::{Path, PathBuf};

use desktop_entry::{
    find_in_path, parse_desktop_entry, visible_entries, DesktopEntry, FilterEnv, LocatedEntry,
};
use launch_core::{plan_launch, LaunchPlanError, LaunchRequest};

/// One display-ready launcher tile derived from a desktop entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub desktop_id: String,
    pub name: String,
    pub comment: String,
    pub icon: String,
    pub exec_preview: String,
    pub terminal: bool,
    /// True for the built-in fallback tiles used when no real desktop files
    /// were found anywhere (clearly labelled in the UI).
    pub demo: bool,
}

/// XDG base-directory `applications` roots in priority order.
///
/// `data_home` defaults to `$HOME/.local/share` and `data_dirs` to
/// `/usr/local/share:/usr/share` per the basedir spec; every component is
/// joined with `applications`.
pub fn applications_roots(
    data_home: Option<&str>,
    data_dirs: Option<&str>,
    home: &str,
) -> Vec<PathBuf> {
    let mut roots = vec![match data_home {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home).join(".local/share"),
    }];
    for part in data_dirs
        .unwrap_or("/usr/local/share:/usr/share")
        .split(':')
        .filter(|part| !part.is_empty())
    {
        roots.push(PathBuf::from(part));
    }
    roots.iter().map(|root| root.join("applications")).collect()
}

/// Roots resolved from the process environment, keeping existing dirs only.
pub fn env_roots() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
    let data_home = std::env::var("XDG_DATA_HOME").ok();
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok();
    applications_roots(data_home.as_deref(), data_dirs.as_deref(), &home)
        .into_iter()
        .filter(|root| root.is_dir())
        .collect()
}

/// Parse every `*.desktop` file directly inside `roots` (flat scan; nested
/// directories are a menu-spec concern and stay out of M0). Unreadable or
/// unparseable files are skipped; `priority` follows root order (lower wins).
pub fn scan_roots(roots: &[PathBuf]) -> Vec<LocatedEntry> {
    let mut entries = Vec::new();
    for (priority, root) in roots.iter().enumerate() {
        let Ok(read_dir) = fs::read_dir(root) else {
            continue;
        };
        for file in read_dir.flatten() {
            let path = file.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            let Some(desktop_id) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(entry) = parse_desktop_entry(&text) {
                entries.push(LocatedEntry {
                    entry,
                    desktop_id: desktop_id.to_string(),
                    source_path: path,
                    priority: priority as u8,
                });
            }
        }
    }
    entries
}

/// Resolve, visibility-filter and sort scanned entries into launcher tiles.
/// `current_desktops` mirrors `$XDG_CURRENT_DESKTOP`; `path_env` drives
/// `TryExec` resolution (`None` means no PATH, so nothing resolves).
pub fn index_visible(
    scanned: &[LocatedEntry],
    current_desktops: &[String],
    path_env: Option<&str>,
) -> Vec<AppEntry> {
    let try_exec_available = |name: &str| find_in_path(name, path_env).is_some();
    let env = FilterEnv {
        current_desktops: current_desktops.to_vec(),
        try_exec_available,
    };
    let mut tiles: Vec<AppEntry> = visible_entries(scanned, &env)
        .into_iter()
        .map(app_tile)
        .filter(|tile| !tile.name.is_empty())
        .collect();
    tiles.sort_by_key(|a| a.name.to_lowercase());
    tiles
}

/// Built-in fallback tiles for environments without any desktop files, so
/// the shell always renders a meaningful, clearly-labelled launcher.
pub fn fallback_entries() -> Vec<AppEntry> {
    const FALLBACK: [(&str, &str, &str, &str, bool); 4] = [
        (
            "demo.files",
            "Files",
            "Browse the filesystem (demo tile)",
            "m0-files %F",
            false,
        ),
        (
            "demo.terminal",
            "Terminal",
            "Command line (demo tile, Terminal=true)",
            "m0-terminal",
            true,
        ),
        (
            "demo.editor",
            "Editor",
            "Simple text editing (demo tile)",
            "m0-edit %f",
            false,
        ),
        (
            "demo.settings",
            "Settings",
            "Session preferences (demo tile)",
            "m0-settings",
            false,
        ),
    ];
    FALLBACK
        .iter()
        .map(|(id, name, comment, exec, terminal)| AppEntry {
            desktop_id: (*id).to_string(),
            name: (*name).to_string(),
            comment: (*comment).to_string(),
            icon: String::new(),
            exec_preview: format!("{exec}   (fallback demo; no desktop file)"),
            terminal: *terminal,
            demo: true,
        })
        .collect()
}

fn app_tile(located: &LocatedEntry) -> AppEntry {
    let entry = &located.entry;
    AppEntry {
        desktop_id: located.desktop_id.clone(),
        name: entry.name.get(None).unwrap_or_default().to_string(),
        comment: entry.comment.get(None).unwrap_or_default().to_string(),
        icon: entry.icon.clone().unwrap_or_default(),
        exec_preview: exec_preview(entry, &located.source_path),
        terminal: entry.terminal,
        demo: false,
    }
}

/// Human-readable preview of the launch plan (argv after field-code
/// expansion). Terminal-only entries keep the raw `Exec` plus a hint because
/// launch-core deliberately has no terminal adapter in M0.
fn exec_preview(entry: &DesktopEntry, source: &Path) -> String {
    let Some(exec) = entry.exec.as_deref() else {
        return "(no Exec)".to_string();
    };
    let request = LaunchRequest {
        exec,
        terminal: entry.terminal,
        files: Vec::new(),
        icon: entry.icon.as_deref(),
        display_name: entry.name.get(None),
        desktop_file_path: source.to_str(),
        activation: None,
        working_directory: entry.path.as_deref().map(Path::new),
    };
    match plan_launch(&request) {
        Ok(plan) => quote_join(&plan.argv),
        Err(LaunchPlanError::UnsupportedTerminal { .. }) => {
            format!("{exec}   (terminal apps need an adapter; not in M0)")
        }
        Err(_) => format!("{exec}   (unexpandable Exec)"),
    }
}

/// Display-only argv join: pieces containing whitespace get single quotes.
fn quote_join(argv: &[String]) -> String {
    argv.iter()
        .map(|piece| {
            if piece.contains(|c: char| c.is_whitespace()) {
                format!("'{piece}'")
            } else {
                piece.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("m0-app-index-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn write_entry(dir: &Path, file: &str, body: &str) {
        fs::write(dir.join(file), body).expect("write fixture");
    }

    const EDITOR: &str = "[Desktop Entry]\nType=Application\nName=Editor\nComment=Edit text\nExec=m0-edit %f\nIcon=accessories-text-editor\n";
    const FILES: &str = "[Desktop Entry]\nType=Application\nName=Files\nComment=Browse\nExec=m0-files %F\nIcon=system-file-manager\n";

    #[test]
    fn scan_filter_sort_pipeline() {
        let dir = scratch_dir("pipeline");
        write_entry(&dir, "org.m0.Editor.desktop", EDITOR);
        write_entry(&dir, "org.m0.Files.desktop", FILES);
        write_entry(
            &dir,
            "org.m0.Hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\nExec=x\n",
        );
        write_entry(
            &dir,
            "org.m0.Link.desktop",
            "[Desktop Entry]\nType=Link\nName=Link\nURL=https://example.com\n",
        );
        fs::write(dir.join("org.m0.Broken.desktop"), [0xff_u8, 0xfe, 0x00]).expect("write broken");

        let scanned = scan_roots(std::slice::from_ref(&dir));
        // The invalid-UTF-8 file is skipped at read time, so 4 parse attempts remain.
        assert_eq!(scanned.len(), 4);

        let tiles = index_visible(&scanned, &[], Some("/usr/bin:/bin"));
        let names: Vec<&str> = tiles.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            ["Editor", "Files"],
            "NoDisplay/Link filtered, sorted"
        );
        assert!(tiles.iter().all(|t| !t.demo));

        let files = tiles.iter().find(|t| t.name == "Files").unwrap();
        // %F with no files expands to zero arguments.
        assert_eq!(files.exec_preview, "m0-files");
        assert_eq!(files.icon, "system-file-manager");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_show_in_respects_current_desktop() {
        let dir = scratch_dir("onlyshowin");
        let body = "[Desktop Entry]\nType=Application\nName=Restricted\nExec=x\nOnlyShowIn=XFCE;\n";
        write_entry(&dir, "org.m0.Restricted.desktop", body);

        let scanned = scan_roots(std::slice::from_ref(&dir));
        assert_eq!(
            index_visible(&scanned, &[], None).len(),
            0,
            "unset variable matches nothing"
        );
        let xfce = ["XFCE".to_string()];
        assert_eq!(index_visible(&scanned, &xfce, None).len(), 1);
        let gnome = ["GNOME".to_string()];
        assert_eq!(index_visible(&scanned, &gnome, None).len(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_exec_resolves_against_path() {
        let dir = scratch_dir("tryexec");
        let body = "[Desktop Entry]\nType=Application\nName=Missing\nExec=x\nTryExec=definitely-not-a-real-m0-binary\n";
        write_entry(&dir, "org.m0.Missing.desktop", body);

        let scanned = scan_roots(std::slice::from_ref(&dir));
        assert_eq!(
            index_visible(&scanned, &[], Some("/usr/bin:/bin")).len(),
            0,
            "TryExec missing"
        );
        assert_eq!(index_visible(&scanned, &[], Some("/usr/bin")).len(), 0);
        // /bin is a symlink to /usr/bin on modern systems; either way it never
        // contains the fabricated binary name.

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminal_entries_get_hint_preview() {
        let dir = scratch_dir("terminal");
        let body =
            "[Desktop Entry]\nType=Application\nName=Term\nExec=m0-term --login\nTerminal=true\n";
        write_entry(&dir, "org.m0.Term.desktop", body);

        let tiles = index_visible(&scan_roots(std::slice::from_ref(&dir)), &[], None);
        assert_eq!(tiles.len(), 1);
        assert!(tiles[0].terminal);
        assert!(tiles[0]
            .exec_preview
            .contains("terminal apps need an adapter"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicates_resolve_by_root_priority() {
        let first = scratch_dir("dup-first");
        let second = scratch_dir("dup-second");
        write_entry(
            &first,
            "org.m0.Dup.desktop",
            "[Desktop Entry]\nType=Application\nName=FromFirst\nExec=a\n",
        );
        write_entry(
            &second,
            "org.m0.Dup.desktop",
            "[Desktop Entry]\nType=Application\nName=FromSecond\nExec=b\n",
        );

        let tiles = index_visible(&scan_roots(&[first.clone(), second.clone()]), &[], None);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].name, "FromFirst", "lower priority root wins");

        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
    }

    #[test]
    fn fallback_tiles_are_labelled_demo() {
        let tiles = fallback_entries();
        assert_eq!(tiles.len(), 4);
        assert!(tiles.iter().all(|tile| tile.demo));
        assert!(tiles.iter().any(|tile| tile.terminal));
    }

    #[test]
    fn applications_roots_follow_basedir_defaults() {
        let roots = applications_roots(None, None, "/home/u");
        assert_eq!(
            roots,
            [
                std::path::PathBuf::from("/home/u/.local/share/applications"),
                std::path::PathBuf::from("/usr/local/share/applications"),
                std::path::PathBuf::from("/usr/share/applications")
            ]
        );
        let custom = applications_roots(Some("/d/h"), Some("/a:/b:"), "/home/u");
        assert_eq!(
            custom,
            [
                std::path::PathBuf::from("/d/h/applications"),
                std::path::PathBuf::from("/a/applications"),
                std::path::PathBuf::from("/b/applications")
            ]
        );
    }
}
