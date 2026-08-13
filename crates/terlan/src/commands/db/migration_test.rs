pub(super) use super::migration::{
    applied_migration_from_history_row, discover_migration_files, load_migration_files,
    migration_history_insert_sql, migration_history_select_sql, migration_history_table_sql,
    pending_migration_engine_inputs, split_migration_sections, AppliedMigration,
    MigrationDiagnostic, MIGRATION_HISTORY_TABLE,
};
pub(super) use std::fs;
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "migration_test/filename_and_discovery.rs"]
mod filename_and_discovery;
#[cfg(test)]
#[path = "migration_test/history_and_engine.rs"]
mod history_and_engine;
use history_and_engine::*;
