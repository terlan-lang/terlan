pub(super) use super::migration::{MigrationStatusEntry, MigrationStatusState};
pub(super) use super::test_support::{remove_dir, temp_db_dir};
pub(super) use super::{run, MigrationStatusSummary};
pub(super) use crate::CliCommand;
pub(super) use std::fs;
pub(super) use std::process::ExitCode;

#[cfg(test)]
#[path = "mod_test/argument_parsing.rs"]
mod argument_parsing;
#[cfg(test)]
#[path = "mod_test/execution_safety.rs"]
mod execution_safety;
