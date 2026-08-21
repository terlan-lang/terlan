//! Local mirror yank command used by Registry self-validation.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::package_registry::model::YankReason;

pub(super) fn run(args: &[String]) -> ExitCode {
    match parse_args(args)
        .map_err(|error| error.to_string())
        .and_then(|command| {
            super::package_registry_mirror::yank_in_mirror(
                &command.mirror,
                &command.package,
                &command.version,
                command.reason_class,
                &command.message,
                command.replacement.as_deref(),
            )
        }) {
        Ok(summary) => {
            println!(
                "yanked {}@{} at sequence {} (snapshot sha256:{})",
                summary.package, summary.version, summary.sequence, summary.snapshot_sha256
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

struct YankCommand {
    mirror: PathBuf,
    package: String,
    version: String,
    reason_class: YankReason,
    message: String,
    replacement: Option<String>,
}

#[derive(Debug)]
struct YankArgumentError(String);

impl std::fmt::Display for YankArgumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for YankArgumentError {}

fn parse_args(args: &[String]) -> Result<YankCommand, YankArgumentError> {
    if args.first().map(String::as_str) != Some("yank") {
        return Err(YankArgumentError(usage()));
    }
    let mut mirror = None;
    let mut package = None;
    let mut version = None;
    let mut reason_class = YankReason::Other;
    let mut message = "operator yank".to_string();
    let mut replacement = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| YankArgumentError(usage()))?
            .clone();
        match flag {
            "--mirror" => mirror = Some(PathBuf::from(value)),
            "--package" => package = Some(value),
            "--version" => version = Some(value),
            "--reason-class" => reason_class = parse_reason_class(&value)?,
            "--message" | "--reason" => message = value,
            "--replacement" => replacement = Some(value),
            _ => return Err(YankArgumentError(usage())),
        }
        index += 1;
    }
    Ok(YankCommand {
        mirror: mirror.ok_or_else(|| YankArgumentError(usage()))?,
        package: package.ok_or_else(|| YankArgumentError(usage()))?,
        version: version.ok_or_else(|| YankArgumentError(usage()))?,
        reason_class,
        message,
        replacement,
    })
}

fn parse_reason_class(value: &str) -> Result<YankReason, YankArgumentError> {
    match value {
        "security" => Ok(YankReason::Security),
        "invalid-metadata" => Ok(YankReason::InvalidMetadata),
        "deprecated" => Ok(YankReason::Deprecated),
        "renamed" => Ok(YankReason::Renamed),
        "other" => Ok(YankReason::Other),
        _ => Err(YankArgumentError(format!(
            "error[registry_yank_reason]: unknown yank reason class `{value}`"
        ))),
    }
}

fn usage() -> String {
    "usage: terlc package yank --mirror <dir> --package <name> --version <version> [--reason-class <security|invalid-metadata|deprecated|renamed|other>] [--message <text>] [--replacement <package>]"
        .into()
}

#[cfg(test)]
#[path = "package_registry_yank_test.rs"]
mod tests;
