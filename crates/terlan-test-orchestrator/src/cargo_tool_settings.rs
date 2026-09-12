//! Narrow, source-aware projection of Cargo's build-tool and subprocess settings.

use crate::execution_environment::{command_value, ExecutionEnvironment};
use crate::PhaseFailure;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const TOOLS: [(&str, &str); 4] = [
    ("rustc", "RUSTC"),
    ("rustdoc", "RUSTDOC"),
    ("rustc-wrapper", "RUSTC_WRAPPER"),
    ("rustc-workspace-wrapper", "RUSTC_WORKSPACE_WRAPPER"),
];

/// Only settings relevant to declared Rust tools and their subprocess environment.
#[derive(Clone, Default)]
pub(super) struct CargoToolSettings {
    build: BTreeMap<String, Setting>,
    environment: BTreeMap<String, Setting>,
}

#[derive(Clone)]
enum Setting {
    Leaf(toml::Value, PathBuf),
    Table(BTreeMap<String, Setting>, PathBuf),
}

impl CargoToolSettings {
    /// Selects an observer while preserving the RUSTDOC environment seen by Cargo's children.
    pub(super) fn observe_rustdoc(
        &self,
        command: &mut Command,
        environment: &ExecutionEnvironment,
        observer: &Path,
    ) -> Result<(), PhaseFailure> {
        command.arg("--config").arg(format!(
            "build.rustdoc={}",
            serde_json::to_string(
                observer
                    .to_str()
                    .ok_or_else(|| failure("Rustdoc observer path is not UTF-8"))?
            )
            .map_err(failure)?
        ));
        if environment.value("RUSTDOC").is_some() {
            let child = self.subprocess_command(environment)?;
            let value = command_value(&child, "RUSTDOC")
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| failure("Rustdoc child environment is not UTF-8"))?;
            let value = serde_json::to_string(&value).map_err(failure)?;
            command.env_remove("RUSTDOC");
            match self.environment.get("RUSTDOC") {
                Some(Setting::Table(_, _)) => {
                    command
                        .arg("--config")
                        .arg(format!("env.RUSTDOC.value={value}"));
                    command.args([
                        "--config",
                        "env.RUSTDOC.force=true",
                        "--config",
                        "env.RUSTDOC.relative=false",
                    ]);
                }
                _ => {
                    command.arg("--config").arg(format!("env.RUSTDOC={value}"));
                }
            }
        }
        Ok(())
    }

    /// Projects one already parsed document without reopening or reparsing it.
    pub(super) fn from_document(path: &Path, document: &toml::Value) -> Result<Self, PhaseFailure> {
        let mut result = Self::default();
        if let Some(build) = document.get("build") {
            let build = build
                .as_table()
                .ok_or_else(|| failure("Cargo build must be a table"))?;
            for (key, _) in TOOLS {
                if let Some(value) = build.get(key) {
                    result
                        .build
                        .insert(key.to_owned(), Setting::new(value, path));
                }
            }
        }
        if let Some(environment) = document.get("env") {
            let environment = environment
                .as_table()
                .ok_or_else(|| failure("Cargo env must be a table"))?;
            if environment.len() > 4_096 {
                return Err(failure("Cargo environment exceeds its entry budget"));
            }
            for (key, value) in environment {
                result
                    .environment
                    .insert(key.clone(), Setting::new(value, path));
            }
        }
        Ok(result)
    }

    /// Includes override left to right; hierarchy roots retain the nearer values.
    pub(super) fn merge(&mut self, other: Self, prefer_new: bool) -> Result<(), PhaseFailure> {
        merge_map(&mut self.build, other.build, prefer_new)?;
        merge_map(&mut self.environment, other.environment, prefer_new)?;
        if self.environment.len() > 4_096 {
            return Err(failure("Cargo environment exceeds its entry budget"));
        }
        Ok(())
    }

    /// Resolves explicit selections only; an absent tool still uses Cargo's default.
    pub(super) fn programs(
        &self,
        environment: &ExecutionEnvironment,
    ) -> Result<Vec<(&'static str, PathBuf)>, PhaseFailure> {
        let directory = home::env::Env::current_dir(environment).map_err(failure)?;
        let command = environment.test_command(Path::new("cargo-environment-observation"));
        let mut programs = Vec::new();
        for (key, variable) in TOOLS {
            // Cargo deserializes build settings before applying RUSTC overrides.
            let configured = self.build.get(key).map(Setting::string).transpose()?;
            let config_environment = command.get_envs().find_map(|(name, value)| {
                (name == format!("CARGO_BUILD_{variable}").as_str())
                    .then_some(value?)
                    .and_then(|value| value.to_str())
            });
            let direct =
                command_value(&command, variable).and_then(|value| value.into_string().ok());
            let selection = if let Some(value) = direct {
                Some(program_path(&value, &directory, true))
            } else if let Some(value) = config_environment {
                Some(program_path(value, &directory, false))
            } else if let Some((value, origin)) = configured {
                Some(program_path(value, config_root(origin)?, false))
            } else {
                None
            };
            if let Some(program) = selection {
                if program.as_os_str().is_empty() && key.contains("wrapper") {
                    continue;
                }
                if program.as_os_str().is_empty() {
                    return Err(failure("configured Cargo compiler or rustdoc is empty"));
                }
                programs.push((variable, program));
            }
        }
        Ok(programs)
    }

    /// Applies Cargo's env table to a frozen Cargo subprocess environment.
    pub(super) fn subprocess_command(
        &self,
        environment: &ExecutionEnvironment,
    ) -> Result<Command, PhaseFailure> {
        self.subprocess_from_command(
            environment.test_command(Path::new("cargo-subprocess-observation")),
        )
    }

    /// Applies env settings after the caller has established Cargo's actual PATH.
    pub(super) fn subprocess_from_command(
        &self,
        mut command: Command,
    ) -> Result<Command, PhaseFailure> {
        let mut inherited = Command::new("cargo-environment-observation");
        inherited.env_clear().envs(
            command
                .get_envs()
                .filter_map(|(key, value)| Some((key, value?))),
        );
        let mut bytes = 0;
        for (key, setting) in &self.environment {
            if key.is_empty() || key.contains(['=', '\0']) {
                return Err(failure("Cargo env contains an invalid variable name"));
            }
            if ["CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN"].contains(&key.as_str()) {
                return Err(failure(
                    "Cargo forbids tool-home and toolchain settings in env",
                ));
            }
            let (value, force, relative, origin) = setting.environment()?;
            if value.contains('\0') {
                return Err(failure("Cargo env contains an invalid variable value"));
            }
            let value = if relative {
                config_root(origin)?.join(value).into_os_string()
            } else {
                value.into()
            };
            bytes += key.len() + value.as_encoded_bytes().len();
            if bytes > 1024 * 1024 {
                return Err(failure("Cargo environment exceeds its byte budget"));
            }
            if force || command_value(&inherited, key).is_none() {
                command.env(key, value);
            }
        }
        Ok(command)
    }
}

impl Setting {
    fn new(value: &toml::Value, origin: &Path) -> Self {
        match value {
            toml::Value::Table(values) => Self::Table(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::new(value, origin)))
                    .collect(),
                origin.to_owned(),
            ),
            value => Self::Leaf(value.clone(), origin.to_owned()),
        }
    }

    fn merge(&mut self, other: Self, prefer_new: bool) -> Result<(), PhaseFailure> {
        match (self, other) {
            (Self::Table(old, _), Self::Table(new, _)) => merge_map(old, new, prefer_new),
            (Self::Leaf(old, _), Self::Leaf(new, _)) if old.is_array() || new.is_array() => Err(
                failure("Cargo tool/env arrays cannot be merged as scalar settings"),
            ),
            (old @ Self::Leaf(..), new @ Self::Leaf(..)) => {
                if prefer_new {
                    *old = new;
                }
                Ok(())
            }
            _ => Err(failure(
                "Cargo tool/env configuration mixes tables and scalars",
            )),
        }
    }

    fn string(&self) -> Result<(&str, &Path), PhaseFailure> {
        match self {
            Self::Leaf(toml::Value::String(value), origin) => Ok((value, origin)),
            _ => Err(failure("Cargo tool setting must be a string")),
        }
    }

    fn environment(&self) -> Result<(&str, bool, bool, &Path), PhaseFailure> {
        match self {
            Self::Leaf(toml::Value::String(value), origin) => Ok((value, false, false, origin)),
            Self::Table(values, origin) => {
                let (value, _) = values
                    .get("value")
                    .ok_or_else(|| failure("Cargo env table requires value"))?
                    .string()?;
                let boolean = |key| match values.get(key) {
                    None => Ok(false),
                    Some(Self::Leaf(toml::Value::Boolean(value), _)) => Ok(*value),
                    _ => Err(failure("Cargo env force/relative must be Boolean")),
                };
                Ok((value, boolean("force")?, boolean("relative")?, origin))
            }
            _ => Err(failure("Cargo env requires a string or options table")),
        }
    }
}

fn merge_map(
    old: &mut BTreeMap<String, Setting>,
    new: BTreeMap<String, Setting>,
    prefer_new: bool,
) -> Result<(), PhaseFailure> {
    for (key, value) in new {
        if let Some(previous) = old.get_mut(&key) {
            previous.merge(value, prefer_new)?;
        } else {
            old.insert(key, value);
        }
    }
    Ok(())
}

fn config_root(origin: &Path) -> Result<&Path, PhaseFailure> {
    origin
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| failure("Cargo configuration has no base directory"))
}

fn program_path(value: &str, base: &Path, direct_environment: bool) -> PathBuf {
    if value.contains('/') || ((direct_environment || cfg!(windows)) && value.contains('\\')) {
        base.join(value)
    } else {
        PathBuf::from(value)
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-tool-selection-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_tool_settings_test.rs"]
mod tests;
