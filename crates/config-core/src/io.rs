//! `load`/`save`: the only I/O surface of this crate.
//!
//! `save` writes a sibling temp file, flushes, fsyncs, then atomically renames
//! over the target (and best-effort fsyncs the directory) so a crash never
//! leaves a half-written config. There is no daemon: callers own when to load
//! or save.

use crate::{classify_message, migrate, validate, Config, ConfigError, CURRENT_SCHEMA_VERSION};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Parse TOML text into a validated [`Config`], applying migrations as needed.
pub fn parse(text: &str) -> Result<Config, ConfigError> {
    let table: toml::Table = toml::from_str(text).map_err(|e| ConfigError::Parse {
        detail: e.to_string(),
    })?;
    let version = migrate::extract_schema_version(&table)?;
    let migrated = migrate::migrate_to_current(table, version)?;
    let normalized = toml::to_string(&migrated).map_err(|e| ConfigError::Parse {
        detail: format!("re-serialize failed: {e}"),
    })?;
    let mut cfg: Config =
        toml::from_str(&normalized).map_err(|e| classify_message(e.to_string()))?;
    // Documents created before a migration step may still miss fields.
    cfg.schema_version = CURRENT_SCHEMA_VERSION;
    validate(&cfg)?;
    Ok(cfg)
}

/// Load and validate the config file at `path`.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = fs::read_to_string(path).map_err(|e| ConfigError::Io {
        path: path.display().to_string(),
        source: e.to_string(),
    })?;
    parse(&text)
}

/// Serialize, validate and atomically replace the config file at `path`.
///
/// Refuses configs whose `schema_version` is newer than this build so an old
/// process can never clobber a newer file's unknown fields.
pub fn save(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    if cfg.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(ConfigError::FutureVersion {
            found: cfg.schema_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    if cfg.schema_version < CURRENT_SCHEMA_VERSION {
        return Err(ConfigError::UnknownVersion {
            found: cfg.schema_version,
        });
    }
    validate(cfg)?;
    let text = toml::to_string_pretty(cfg).map_err(|e| ConfigError::Io {
        path: path.display().to_string(),
        source: format!("serialize failed: {e}"),
    })?;
    atomic_write(path, text.as_bytes())
}

/// Same-directory temp file + flush + fsync + rename, then fsync the parent
/// directory (best effort: not every filesystem supports dir fsync).
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".to_string());
    let mut tmp: PathBuf = dir.join(format!(".{file_name}.tmp.{}", std::process::id()));
    let mut attempt = 0u32;
    loop {
        let write_result = (|| {
            let mut out = fs::File::create(&tmp).map_err(|e| ConfigError::Io {
                path: tmp.display().to_string(),
                source: e.to_string(),
            })?;
            out.write_all(bytes).map_err(|e| ConfigError::Io {
                path: tmp.display().to_string(),
                source: e.to_string(),
            })?;
            out.flush().map_err(|e| ConfigError::Io {
                path: tmp.display().to_string(),
                source: e.to_string(),
            })?;
            out.sync_all().map_err(|e| ConfigError::Io {
                path: tmp.display().to_string(),
                source: e.to_string(),
            })
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        match fs::rename(&tmp, path) {
            Ok(()) => break,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                // Retry once on a name collision with a concurrent writer.
                attempt += 1;
                if attempt >= 2 {
                    return Err(ConfigError::Io {
                        path: path.display().to_string(),
                        source: format!("atomic rename failed: {e}"),
                    });
                }
                tmp = dir.join(format!(
                    ".{file_name}.tmp.{}/{}",
                    std::process::id(),
                    attempt
                ));
            }
        }
    }
    if let Ok(dir_handle) = fs::File::open(dir) {
        // Best effort; failure to fsync the directory is not fatal.
        let _ = dir_handle.sync_all();
    }
    Ok(())
}
