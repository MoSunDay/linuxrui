//! Offline unit tests for tokenization semantics surfaced through
//! `plan_launch`: quoting, escaping, file-list spreading, icon/name codes,
//! deprecated and unknown codes, and the explicit terminal refusal.

use crate::{plan_launch, LaunchFile, LaunchPlanError, LaunchRequest, TokenKind};

fn plan(exec: &str) -> LaunchRequest<'_> {
    LaunchRequest {
        exec,
        ..Default::default()
    }
}

fn argv(exec: &str) -> Vec<String> {
    plan_launch(&plan(exec)).expect("plan").argv
}

#[test]
fn quoting_and_escaping_survive_into_argv() {
    assert_eq!(argv("prog"), ["prog"]);
    assert_eq!(argv("prog \"one two\" three"), ["prog", "one two", "three"]);
    assert_eq!(argv("prog --title=\"My App\""), ["prog", "--title=My App"]);
    assert_eq!(
        argv("prog a\\ b \\'q\\' \\\\"),
        ["prog", "a b", "'q'", "\\"]
    );
}

#[test]
fn single_file_codes_take_first_file() {
    let mut req = plan("prog --open %f");
    req.files = vec![
        LaunchFile::Path("/tmp/a.txt".into()),
        LaunchFile::Path("/tmp/b.txt".into()),
    ];
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "--open", "/tmp/a.txt"]
    );
    let mut req = plan("prog %u");
    req.files = vec![LaunchFile::Uri("https://example.com/x".into())];
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "https://example.com/x"]
    );
}

#[test]
fn file_list_codes_spread_into_separate_arguments() {
    let mut req = plan("prog --files %F");
    req.files = vec![
        LaunchFile::Path("/a".into()),
        LaunchFile::Path("/b".into()),
        LaunchFile::Path("/c".into()),
    ];
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "--files", "/a", "/b", "/c"]
    );
    let mut req = plan("prog %U");
    req.files = vec![
        LaunchFile::Uri("https://a".into()),
        LaunchFile::Uri("https://b".into()),
    ];
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "https://a", "https://b"]
    );
}

#[test]
fn missing_files_remove_the_code_argument_only() {
    // Per spec only the argument containing the field code is removed;
    // neighbouring flags stay (the spawner may still surface them).
    let req = plan("prog --open %f end");
    assert_eq!(plan_launch(&req).unwrap().argv, ["prog", "--open", "end"]);
    let req = plan("prog --file=%f");
    assert_eq!(plan_launch(&req).unwrap().argv, ["prog"]);
    let req = plan("prog %F");
    assert_eq!(plan_launch(&req).unwrap().argv, ["prog"]);
}

#[test]
fn icon_code_yields_zero_or_two_arguments() {
    let req = plan("prog %i start");
    assert_eq!(plan_launch(&req).unwrap().argv, ["prog", "start"]);
    let mut req = plan("prog %i start");
    req.icon = Some("org.example.Icon");
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "--icon", "org.example.Icon", "start"]
    );
}

#[test]
fn name_code_uses_caller_resolved_localization() {
    let mut req = plan("prog --title %c");
    req.display_name = Some("示例编辑器");
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "--title", "示例编辑器"]
    );
    let req = plan("prog %c");
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog"],
        "missing name drops the arg"
    );
}

#[test]
fn desktop_path_code_uses_location_or_drops() {
    let mut req = plan("prog %k");
    req.desktop_file_path = Some("/usr/share/applications/x.desktop");
    assert_eq!(
        plan_launch(&req).unwrap().argv,
        ["prog", "/usr/share/applications/x.desktop"]
    );
    let req = plan("prog %k");
    assert_eq!(plan_launch(&req).unwrap().argv, ["prog"]);
}

#[test]
fn percent_percent_is_literal() {
    // `%%` collapses to one `%`; it does not escape the following space.
    assert_eq!(argv("prog 100%% of\\ it"), ["prog", "100%", "of it"]);
}

#[test]
fn deprecated_codes_are_deleted() {
    assert_eq!(argv("prog %d %D %n %N %v %m keep"), ["prog", "keep"]);
    assert_eq!(
        argv("prog --icon=%i"),
        ["prog"],
        "embedded icon without Icon drops the arg"
    );
}

#[test]
fn unknown_codes_are_rejected() {
    assert_eq!(
        plan_launch(&plan("prog %z")).unwrap_err(),
        LaunchPlanError::UnknownFieldCode { code: "%z".into() }
    );
    assert_eq!(
        plan_launch(&plan("prog trailing%")).unwrap_err(),
        LaunchPlanError::UnknownFieldCode { code: "%".into() }
    );
}

#[test]
fn terminal_true_is_explicitly_unsupported() {
    let mut req = plan("prog %f");
    req.terminal = true;
    req.display_name = Some("Terminal App");
    match plan_launch(&req).unwrap_err() {
        LaunchPlanError::UnsupportedTerminal { entry_hint } => {
            assert_eq!(entry_hint, "Terminal App");
        }
        other => panic!("expected UnsupportedTerminal, got {other:?}"),
    }
}

#[test]
fn tokenize_and_empty_errors_are_diagnosable() {
    assert!(matches!(
        plan_launch(&plan("prog \"oops")).unwrap_err(),
        LaunchPlanError::Tokenize { .. }
    ));
    assert_eq!(
        plan_launch(&plan("   ")).unwrap_err(),
        LaunchPlanError::EmptyExec
    );
}

#[test]
fn activation_token_metadata_uses_the_right_env_var() {
    let mut req = plan("prog");
    req.activation = Some(("wayland-token-1", TokenKind::XdgActivationToken));
    let planned = plan_launch(&req).unwrap();
    assert_eq!(
        planned
            .activation_env
            .as_ref()
            .map(|(v, k)| (v.as_str(), *k)),
        Some(("wayland-token-1", "XDG_ACTIVATION_TOKEN"))
    );
    let mut req = plan("prog");
    req.activation = Some(("legacy-id", TokenKind::DesktopStartupId));
    let planned = plan_launch(&req).unwrap();
    assert_eq!(
        planned.activation_env.as_ref().map(|(_, k)| *k),
        Some("DESKTOP_STARTUP_ID")
    );
    let planned = plan_launch(&plan("prog")).unwrap();
    assert!(planned.activation_env.is_none());
}

#[test]
fn working_directory_is_carried_into_the_plan() {
    let mut req = plan("prog");
    req.working_directory = Some(std::path::Path::new("/tmp"));
    let planned = plan_launch(&req).unwrap();
    assert_eq!(
        planned.working_directory.as_deref(),
        Some(std::path::Path::new("/tmp"))
    );
}
