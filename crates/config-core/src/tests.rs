//! Offline unit tests: valid round-trips, corrupt input, future versions,
//! unknown fields and atomic replace. All file I/O goes to fresh temp dirs.

use crate::io::{load, parse, save};
use crate::{
    Config, ConfigError, NotificationConfig, PanelConfig, PanelPosition, ThemePreference,
    CURRENT_SCHEMA_VERSION,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static DIR_SEQ: AtomicU32 = AtomicU32::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let seq = DIR_SEQ.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("config-core-{}-{}-{seq}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn sample() -> Config {
    Config {
        schema_version: CURRENT_SCHEMA_VERSION,
        appearance: crate::AppearanceConfig {
            theme: ThemePreference::Dark,
            icon_theme: "Papirus".into(),
        },
        animation: crate::AnimationConfig {
            enable_animations: false,
            duration_ms: 90,
        },
        panel: PanelConfig {
            position: PanelPosition::Top,
            height_px: 32,
            modules: vec!["window-list".into(), "clock".into()],
        },
        launcher: crate::LauncherConfig {
            remember_search_history: true,
        },
        notification: NotificationConfig::default(),
    }
}

#[test]
fn valid_document_parses_with_defaults() {
    let cfg = parse("schema_version = 1\n").expect("minimal doc parses");
    assert_eq!(cfg.schema_version, 1);
    assert_eq!(cfg.appearance.theme, ThemePreference::Auto);
    assert_eq!(cfg.panel.position, PanelPosition::Bottom);
    assert!(!cfg.launcher.remember_search_history);
    assert!(cfg.notification.privacy.show_body);
}

#[test]
fn save_then_load_round_trips() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("desktop.toml");
    save(&path, &sample()).expect("save");
    let loaded = load(&path).expect("load after save");
    assert_eq!(loaded, sample());
}

#[test]
fn atomic_replace_is_readable_and_leaves_no_temp_files() {
    let dir = temp_dir("atomic");
    let path = dir.join("desktop.toml");
    save(&path, &sample()).expect("first save");
    let mut next = sample();
    next.panel.height_px = 40;
    next.appearance.theme = ThemePreference::Light;
    save(&path, &next).expect("second save replaces atomically");
    let loaded = load(&path).expect("load after atomic replace");
    assert_eq!(loaded, next);
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .expect("list dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files must be renamed away, found {leftovers:?}"
    );
    assert_eq!(fs::read_dir(&dir).expect("list dir").count(), 1);
}

#[test]
fn corrupt_toml_is_a_parse_error() {
    let err = parse("schema_version = 1\n[panel\nbroken").unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }), "got {err:?}");
}

#[test]
fn future_version_is_read_only() {
    let err = parse("schema_version = 2\n[panel]\nheight_px = 10\n").unwrap_err();
    assert_eq!(
        err,
        ConfigError::FutureVersion {
            found: 2,
            supported: 1
        }
    );
    let text = err.to_string();
    assert!(
        text.contains("read-only"),
        "error must explain read-only protection: {text}"
    );
}

#[test]
fn missing_schema_version_is_explicit() {
    assert_eq!(
        parse("[panel]\nheight_px = 10\n").unwrap_err(),
        ConfigError::MissingSchemaVersion
    );
}

#[test]
fn non_integer_schema_version_is_explicit() {
    let err = parse("schema_version = \"one\"\n").unwrap_err();
    assert!(
        matches!(err, ConfigError::InvalidSchemaVersion { .. }),
        "got {err:?}"
    );
}

#[test]
fn unknown_field_is_explicit() {
    let err = parse("schema_version = 1\n[panel]\nheigth_px = 10\n").unwrap_err();
    assert!(
        matches!(err, ConfigError::UnknownField { .. }),
        "got {err:?}"
    );
    let ConfigError::UnknownField { detail } = err else {
        panic!()
    };
    assert!(
        detail.contains("heigth_px"),
        "detail should name the key: {detail}"
    );
}

#[test]
fn wrong_value_type_is_a_parse_error() {
    let err = parse("schema_version = 1\n[panel]\nposition = \"diagonal\"\n").unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }), "got {err:?}");
}

#[test]
fn out_of_range_values_are_invalid() {
    let mut cfg = sample();
    cfg.animation.duration_ms = 60_000;
    assert!(matches!(
        crate::validate(&cfg).unwrap_err(),
        ConfigError::Invalid { .. }
    ));
    let mut cfg = sample();
    cfg.panel.height_px = 2;
    assert!(matches!(
        crate::validate(&cfg).unwrap_err(),
        ConfigError::Invalid { .. }
    ));
}

#[test]
fn save_refuses_future_version_struct() {
    let dir = temp_dir("refuse-future");
    let path = dir.join("desktop.toml");
    let mut cfg = sample();
    cfg.schema_version = 99;
    assert_eq!(
        save(&path, &cfg).unwrap_err(),
        ConfigError::FutureVersion {
            found: 99,
            supported: 1
        }
    );
    assert!(
        !path.exists(),
        "refused save must not touch the target file"
    );
}
