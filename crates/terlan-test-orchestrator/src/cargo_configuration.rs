//! Cargo configuration discovery over the already hashed, bounded byte snapshot.

use super::{failure, observe_file, ConfigurationFile, MAX_BYTES, MAX_PATHS};
use crate::cargo_tool_settings::CargoToolSettings;
use crate::execution_environment::ExecutionEnvironment;
use crate::{process_failure, PhaseFailure};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

/// Follows active Cargo includes and retains a low-to-high-precedence load order.
pub(super) fn expand(
    environment: &ExecutionEnvironment,
    rows: &mut Vec<ConfigurationFile>,
    control: ProcessControl<'_>,
    started: Instant,
) -> Result<(Vec<PathBuf>, CargoToolSettings), PhaseFailure> {
    let directory = home::env::Env::current_dir(environment).map_err(failure)?;
    let cargo_home = directory.join(home::env::cargo_home_with_env(environment).map_err(failure)?);
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    let mut directories = directory
        .ancestors()
        .map(|path| path.join(".cargo"))
        .collect::<Vec<_>>();
    for root in &directories {
        seen.insert(std::fs::canonicalize(root).unwrap_or_else(|_| root.clone()));
    }
    let canonical_home = std::fs::canonicalize(&cargo_home).unwrap_or_else(|_| cargo_home.clone());
    if !seen.contains(&canonical_home) && !seen.contains(&cargo_home) {
        directories.push(cargo_home);
    }
    for root in directories {
        // Cargo prefers the extensionless name when both alternatives exist.
        let selected = [root.join("config"), root.join("config.toml")]
            .into_iter()
            .find(|path| rows.iter().any(|row| row.path == *path && row.present));
        if let Some(path) = selected {
            if !roots.contains(&path) {
                roots.push(path);
            }
        }
    }
    let mut owner = IncludeInventory {
        remaining: MAX_BYTES - rows.iter().map(|row| row.bytes).sum::<u64>(),
        rows,
        control,
        started,
        order: Vec::new(),
        visits: 0,
    };
    let mut settings = CargoToolSettings::default();
    for root in roots.into_iter().rev() {
        let mut nearer = owner.load(&root, false, &mut BTreeSet::new(), 0)?;
        nearer.merge(settings, false)?;
        settings = nearer;
    }
    Ok((owner.order, settings))
}

struct IncludeInventory<'a, 'control> {
    rows: &'a mut Vec<ConfigurationFile>,
    remaining: u64,
    control: ProcessControl<'control>,
    started: Instant,
    order: Vec<PathBuf>,
    visits: usize,
}

impl IncludeInventory<'_, '_> {
    fn load(
        &mut self,
        path: &Path,
        optional: bool,
        seen: &mut BTreeSet<PathBuf>,
        depth: usize,
    ) -> Result<CargoToolSettings, PhaseFailure> {
        self.control.check(self.started).map_err(process_failure)?;
        self.visits += 1;
        if depth >= 64 || self.visits > MAX_PATHS {
            return Err(failure("Cargo includes exceed their traversal budget"));
        }
        let index = match self.rows.iter().position(|row| row.path == path) {
            Some(index) => index,
            None => {
                if self.rows.len() >= MAX_PATHS {
                    return Err(failure(
                        "Cargo includes exceed the configuration path budget",
                    ));
                }
                let row = observe_file(path, self.control, self.started, self.remaining)?;
                self.remaining -= row.bytes;
                self.rows.push(row);
                self.rows.len() - 1
            }
        };
        let row = &self.rows[index];
        if !row.present {
            return if optional {
                Ok(CargoToolSettings::default())
            } else {
                Err(failure("required Cargo configuration include is missing"))
            };
        }
        // Retain Cargo's per-root repeated-include rejection; depth also bounds
        // aliases whose lexical spelling hides a filesystem cycle.
        if !seen.insert(path.to_owned()) {
            return Err(failure(
                "Cargo configuration contains a repeated or cyclic include",
            ));
        }
        let contents = std::str::from_utf8(&row.contents)
            .map_err(|_| failure("Cargo configuration is not UTF-8"))?;
        // Do not expose parser excerpts: Cargo config can contain credentials.
        let document: toml::Value = toml::from_str(contents)
            .map_err(|_| failure("Cargo configuration is not valid TOML"))?;
        let includes = include_paths(path, &document)?;
        let own_settings = CargoToolSettings::from_document(path, &document)?;
        drop(document);
        let mut settings = CargoToolSettings::default();
        for (included, optional) in includes {
            settings.merge(self.load(&included, optional, seen, depth + 1)?, true)?;
        }
        settings.merge(own_settings, true)?;
        self.order.push(path.to_owned());
        Ok(settings)
    }
}

fn include_paths(
    path: &Path,
    document: &toml::Value,
) -> Result<Vec<(PathBuf, bool)>, PhaseFailure> {
    let Some(include) = document.get("include") else {
        return Ok(Vec::new());
    };
    let includes = include
        .as_array()
        .filter(|values| values.len() <= MAX_PATHS)
        .ok_or_else(|| failure("Cargo include must be a bounded array"))?;
    includes
        .iter()
        .map(|include| {
            let (value, optional) = if let Some(value) = include.as_str() {
                (value, false)
            } else {
                let value = include
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .ok_or_else(|| failure("Cargo include table requires a string path"))?;
                let optional = include
                    .get("optional")
                    .map(|value| {
                        value
                            .as_bool()
                            .ok_or_else(|| failure("Cargo include optional must be Boolean"))
                    })
                    .transpose()?
                    .unwrap_or(false);
                (value, optional)
            };
            if Path::new(value).extension() != Some(std::ffi::OsStr::new("toml"))
                || value.contains(['*', '?', '[', ']', '{', '}'])
            {
                return Err(failure("Cargo include requires a literal .toml path"));
            }
            let parent = path
                .parent()
                .ok_or_else(|| failure("Cargo config has no parent"))?;
            Ok((parent.join(value), optional))
        })
        .collect()
}

#[cfg(test)]
#[path = "cargo_configuration_test.rs"]
mod tests;
