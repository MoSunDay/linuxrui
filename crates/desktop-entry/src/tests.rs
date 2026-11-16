//! Offline fixture tests: parsing, zh_CN localization, Hidden shadowing,
//! OnlyShowIn/NotShowIn, TryExec filtering and invalid-action handling.

use crate::filter::{
    find_in_path, parse_current_desktops, visible_entries, FilterEnv, LocatedEntry,
};
use crate::parse::parse_desktop_entry;
use crate::{DiagnosticKind, EntryType, ParseError};

const ZH_APP: &str = "\
[Desktop Entry]
Type=Application
Name=Example Editor
Name[zh_CN]=示例编辑器
GenericName=Text Editor
GenericName[zh_CN]=文本编辑器
Comment=Edit text files
Comment[zh_CN]=编辑文本文件
Keywords=text;editor;文本;
Exec=example-editor %f
TryExec=example-editor
Icon=example-editor
Path=/tmp
Terminal=false
DBusActivatable=false
Categories=Utility;
";

fn located(text: &str, id: &str, priority: u8) -> LocatedEntry {
    LocatedEntry {
        entry: parse_desktop_entry(text).expect("fixture parses"),
        desktop_id: id.to_string(),
        source_path: format!("/fake/applications/{id}").into(),
        priority,
    }
}

fn env(desktops: &[&str], available: bool) -> FilterEnv<impl Fn(&str) -> bool> {
    FilterEnv {
        current_desktops: desktops.iter().map(|s| s.to_string()).collect(),
        try_exec_available: move |_name| available,
    }
}

#[test]
fn parses_full_document_and_zh_cn_localization() {
    let entry = parse_desktop_entry(ZH_APP).expect("parses");
    assert_eq!(entry.entry_type, Some(EntryType::Application));
    assert_eq!(entry.name.get(Some("zh_CN")), Some("示例编辑器"));
    assert_eq!(entry.generic_name.get(Some("zh_CN")), Some("文本编辑器"));
    assert_eq!(entry.comment.get(Some("zh_CN")), Some("编辑文本文件"));
    assert_eq!(
        entry.keywords.get(Some("zh_CN")),
        ["text", "editor", "文本"]
    );
    assert_eq!(entry.exec.as_deref(), Some("example-editor %f"));
    assert_eq!(entry.icon.as_deref(), Some("example-editor"));
    assert_eq!(entry.path.as_deref(), Some("/tmp"));
    assert!(!entry.terminal && !entry.hidden && !entry.no_display);
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
}

#[test]
fn locale_falls_back_to_unlocalized_name() {
    let entry = parse_desktop_entry(ZH_APP).expect("parses");
    assert_eq!(entry.name.get(Some("fr_FR")), Some("Example Editor"));
    assert_eq!(entry.name.get(None), Some("Example Editor"));
}

#[test]
fn comments_and_ignored_groups_do_not_break_parsing() {
    let text = "# leading comment\n[Desktop Entry]\nType=Application\nName=X\nExec=x\n\n[Other Group]\nFoo=bar\n# trailing comment\n";
    let entry = parse_desktop_entry(text).expect("parses");
    assert!(entry
        .diagnostics
        .iter()
        .any(|d| d.kind == DiagnosticKind::IgnoredGroup));
}

#[test]
fn missing_required_keys_are_fatal() {
    let no_name = "[Desktop Entry]\nType=Application\n";
    assert_eq!(
        parse_desktop_entry(no_name).unwrap_err(),
        ParseError::MissingRequiredKey { key: "Name" }
    );
    let no_type = "[Desktop Entry]\nName=X\n";
    assert_eq!(
        parse_desktop_entry(no_type).unwrap_err(),
        ParseError::MissingRequiredKey { key: "Type" }
    );
    // Keys without a group header never open [Desktop Entry].
    assert_eq!(
        parse_desktop_entry("Type=Application\n").unwrap_err(),
        ParseError::MissingMainGroup
    );
    let no_exec = "[Desktop Entry]\nType=Application\nName=X\n";
    assert_eq!(
        parse_desktop_entry(no_exec).unwrap_err(),
        ParseError::ApplicationWithoutExec
    );
}

#[test]
fn invalid_boolean_is_diagnosed_and_treated_as_false() {
    let text = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nTerminal=yes\n";
    let entry = parse_desktop_entry(text).expect("parses");
    assert!(!entry.terminal);
    assert!(entry
        .diagnostics
        .iter()
        .any(|d| d.kind == DiagnosticKind::InvalidBoolean));
}

#[test]
fn hidden_high_priority_shadows_same_desktop_id() {
    let masked = "[Desktop Entry]\nType=Application\nName=Old\nExec=old\nHidden=true\n";
    let normal = "[Desktop Entry]\nType=Application\nName=New\nExec=new\n";
    let items = vec![
        located(masked, "app.desktop", 0),
        located(normal, "app.desktop", 1),
    ];
    let env = env(&[], true);
    assert!(
        visible_entries(&items, &env).is_empty(),
        "masked ID must not fall through"
    );
    // Reversed priority: the visible one wins.
    let items = vec![
        located(normal, "app.desktop", 0),
        located(masked, "app.desktop", 1),
    ];
    let visible = visible_entries(&items, &env);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].entry.name.default_value(), Some("New"));
}

#[test]
fn only_show_in_matches_colon_separated_desktops() {
    let text = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nOnlyShowIn=GNOME;sway;\n";
    let items = [located(text, "restricted.desktop", 0)];
    assert_eq!(visible_entries(&items, &env(&["sway"], true)).len(), 1);
    assert_eq!(
        visible_entries(&items, &env(&["GNOME", "labwc"], true)).len(),
        1
    );
    assert!(visible_entries(&items, &env(&["KDE"], true)).is_empty());
    assert!(
        visible_entries(&items, &env(&[], true)).is_empty(),
        "unset XDG_CURRENT_DESKTOP hides restricted items"
    );
    assert_eq!(
        parse_current_desktops(Some(" sway :labwc ")),
        ["sway", "labwc"]
    );
    assert!(parse_current_desktops(None).is_empty());
}

#[test]
fn not_show_in_takes_precedence_over_only_show_in() {
    let text =
        "[Desktop Entry]\nType=Application\nName=X\nExec=x\nOnlyShowIn=sway;\nNotShowIn=sway;\n";
    let items = [located(text, "conflict.desktop", 0)];
    assert!(visible_entries(&items, &env(&["sway"], true)).is_empty());
    assert!(visible_entries(&items, &env(&["GNOME"], true)).is_empty());
}

#[test]
fn no_display_and_non_application_types_are_hidden() {
    let no_display = "[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n";
    let link = "[Desktop Entry]\nType=Link\nName=Doc\nURL=https://example.com\n";
    assert!(visible_entries(&[located(no_display, "nd.desktop", 0)], &env(&[], true)).is_empty());
    assert!(visible_entries(&[located(link, "link.desktop", 0)], &env(&[], true)).is_empty());
}

#[test]
fn try_exec_missing_filters_entry_out() {
    let items = [located(ZH_APP, "editor.desktop", 0)];
    assert!(visible_entries(&items, &env(&[], false)).is_empty());
    assert_eq!(visible_entries(&items, &env(&[], true)).len(), 1);
}

#[test]
fn bad_actions_are_dropped_but_main_entry_survives() {
    let text = "\
[Desktop Entry]
Type=Application
Name=Cloud Thing
Exec=cloud-thing
Actions=new-window;bad;

[Desktop Action new-window]
Name=New Window
Name[zh_CN]=新建窗口
Exec=cloud-thing --new-window

[Desktop Action unlisted]
Name=Unlisted
Exec=cloud-thing --unlisted

[Desktop Action bad]
Name=Bad
";
    let entry = parse_desktop_entry(text).expect("main entry survives");
    assert_eq!(entry.actions.len(), 1, "only new-window survives");
    assert_eq!(entry.actions[0].id, "new-window");
    assert_eq!(entry.actions[0].name.get(Some("zh_CN")), Some("新建窗口"));
    let kinds: Vec<_> = entry.diagnostics.iter().map(|d| d.kind).collect();
    assert!(
        kinds.contains(&DiagnosticKind::DroppedAction),
        "bad action dropped: {kinds:?}"
    );
    assert!(
        kinds.contains(&DiagnosticKind::MissingActionGroup),
        "unlisted group diagnosed: {kinds:?}"
    );
}

#[test]
fn invalid_action_group_header_is_ignored() {
    let text = "[Desktop Entry]\nType=Application\nName=X\nExec=x\n[Desktop Action bad id!]\nName=Y\nExec=y\n";
    let entry = parse_desktop_entry(text).expect("parses");
    assert!(entry.actions.is_empty());
    assert!(entry
        .diagnostics
        .iter()
        .any(|d| d.kind == DiagnosticKind::DroppedAction));
}

#[test]
fn duplicate_keys_keep_first_value() {
    let text = "[Desktop Entry]\nType=Application\nName=First\nName=Second\nExec=x\n";
    let entry = parse_desktop_entry(text).expect("parses");
    assert_eq!(entry.name.default_value(), Some("First"));
    assert!(entry
        .diagnostics
        .iter()
        .any(|d| d.kind == DiagnosticKind::DuplicateKey));
}

#[test]
fn find_in_path_resolves_executables() {
    let exe = "/bin/sh";
    let path = find_in_path("sh", Some("/nonexistent:/bin")).expect("found in /bin");
    assert!(path.to_string_lossy().ends_with("sh"));
    assert!(find_in_path("definitely-not-a-real-binary-xyz", Some("/bin")).is_none());
    assert!(
        find_in_path("sh", None).is_none(),
        "empty PATH yields no match"
    );
    assert_eq!(
        find_in_path(exe, Some("")),
        Some(exe.into()),
        "absolute path used as-is"
    );
}
