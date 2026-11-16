//! Pure-function schema migration framework.
//!
//! `MIGRATIONS[i]` upgrades a document from schema version `i + 1` to `i + 2`
//! by mutating the raw TOML table. Documents from the future are never
//! migrated down and never rewritten by this build (read-only protection).

use crate::{ConfigError, CURRENT_SCHEMA_VERSION, MIN_SCHEMA_VERSION};

/// One migration step: `v -> v+1` over the raw table. Steps must be total and
/// deterministic (no I/O, no clock, no randomness) so they are unit-testable.
pub type MigrationFn = fn(&mut toml::Table) -> Result<(), String>;

/// Registered steps. Empty while `CURRENT_SCHEMA_VERSION == 1` (v1 identity).
pub const MIGRATIONS: &[MigrationFn] = &[];

/// Read the top-level `schema_version` without deserializing the whole model.
pub fn extract_schema_version(table: &toml::Table) -> Result<i64, ConfigError> {
    match table.get("schema_version") {
        None => Err(ConfigError::MissingSchemaVersion),
        Some(toml::Value::Integer(v)) if *v >= 0 => Ok(*v),
        Some(other) => Err(ConfigError::InvalidSchemaVersion {
            value: other.to_string(),
        }),
    }
}

/// Migrate a parsed document forward to [`CURRENT_SCHEMA_VERSION`].
///
/// - `from == CURRENT_SCHEMA_VERSION`: identity (no table mutation beyond
///   normalizing the stored version integer).
/// - `from > CURRENT_SCHEMA_VERSION`: refused, callers must treat the file as
///   read-only; this build must not downgrade or silently drop future fields.
/// - `from < MIN_SCHEMA_VERSION`: refused as unknown/garbage.
pub fn migrate_to_current(mut table: toml::Table, from: i64) -> Result<toml::Table, ConfigError> {
    if from < MIN_SCHEMA_VERSION {
        return Err(ConfigError::UnknownVersion { found: from });
    }
    if from > CURRENT_SCHEMA_VERSION {
        return Err(ConfigError::FutureVersion {
            found: from,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    for step in from..CURRENT_SCHEMA_VERSION {
        let migrate = MIGRATIONS
            .get((step - MIN_SCHEMA_VERSION) as usize)
            .ok_or(ConfigError::UnknownVersion { found: step })?;
        migrate(&mut table).map_err(|detail| ConfigError::Parse {
            detail: format!("migration v{step} -> v{} failed: {detail}", step + 1),
        })?;
    }
    table.insert(
        "schema_version".into(),
        toml::Value::Integer(CURRENT_SCHEMA_VERSION),
    );
    Ok(table)
}

#[cfg(test)]
mod step_tests {
    use super::*;

    #[test]
    fn v1_is_identity() {
        let table: toml::Table =
            toml::from_str("schema_version = 1\n[panel]\nheight_px = 42\nmodules = [\"clock\"]\n")
                .expect("fixture parses");
        let migrated = migrate_to_current(table.clone(), 1).expect("v1 migrates");
        assert_eq!(migrated, table);
    }

    #[test]
    fn future_version_is_refused() {
        let err = migrate_to_current(toml::Table::new(), 2).unwrap_err();
        assert_eq!(
            err,
            ConfigError::FutureVersion {
                found: 2,
                supported: CURRENT_SCHEMA_VERSION
            }
        );
    }

    #[test]
    fn zero_version_is_unknown() {
        assert_eq!(
            migrate_to_current(toml::Table::new(), 0).unwrap_err(),
            ConfigError::UnknownVersion { found: 0 }
        );
    }
}
