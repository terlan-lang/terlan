//! Match observed Cargo targets to explicit or conventional libtest declarations.

use crate::PhaseFailure;
use std::path::{Path, PathBuf};
use toml::value::Table;

/// Declared source and harness policy; Cargo still owns compilation and feature selection.
pub(super) struct TargetDeclaration {
    /// Cargo package identity, distinct from normalized library target names.
    pub(super) package: String,
    /// Observed target name matched to the declaration or discovery convention.
    pub(super) name: String,
    /// Admitted executable category; unsupported categories fail closed.
    pub(super) kind: &'static str,
    /// Declared invocation source alias, checked against Cargo's source observation.
    pub(super) source: PathBuf,
    /// Distinguishes explicit libtest selection from Cargo's default policy.
    pub(super) explicit_harness: bool,
}

/// Resolves only the observed target, without scanning directories or launching Cargo again.
pub(super) fn resolve(
    document: &toml::Value,
    target: &serde_json::Value,
    package_root: &Path,
) -> Result<TargetDeclaration, PhaseFailure> {
    let package = document
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| failure("Cargo manifest has no package table"))?;
    let package_name =
        string(package, "name")?.ok_or_else(|| failure("Cargo package has no name"))?;
    let name = target["name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| failure("Cargo target has no name"))?;
    let kinds = target["kind"]
        .as_array()
        .ok_or_else(|| failure("Cargo target has no kind"))?;
    let kind = match kinds.as_slice() {
        [kind] if kind == "lib" => "lib",
        [kind] if kind == "bin" => "bin",
        [kind] if kind == "test" => "test",
        _ => return Err(failure("Cargo target kind has no declared libtest owner")),
    };
    let table = if kind == "lib" {
        let table = document
            .get("lib")
            .map(|value| {
                value
                    .as_table()
                    .ok_or_else(|| failure("Cargo lib declaration is not a table"))
            })
            .transpose()?;
        let declared_name = table
            .map(|table| string(table, "name"))
            .transpose()?
            .flatten()
            .map(str::to_owned)
            .unwrap_or_else(|| package_name.replace('-', "_"));
        if name != declared_name {
            return Err(failure("Cargo artifact and library target name disagree"));
        }
        if table.is_none() && boolean(package, "autolib")? == Some(false) {
            return Err(failure("Cargo library autodiscovery is disabled"));
        }
        table
    } else {
        target_table(document, package, target, kind, name, package_root)?
    };
    let explicit_harness = match table
        .map(|table| boolean(table, "harness"))
        .transpose()?
        .flatten()
    {
        None => false,
        Some(true) => true,
        Some(false) => {
            return Err(failure(format!(
                "custom Cargo {kind} harness requires a declared non-libtest owner"
            )))
        }
    };
    let path = table
        .map(|table| string(table, "path"))
        .transpose()?
        .flatten();
    let source = if let Some(path) = path {
        package_root.join(path)
    } else {
        inferred_source(
            kind,
            name,
            package_name,
            package_root,
            table.is_some(),
            package,
            target,
        )?
    };
    Ok(TargetDeclaration {
        package: package_name.to_owned(),
        name: name.to_owned(),
        kind,
        source,
        explicit_harness,
    })
}

fn target_table<'a>(
    document: &'a toml::Value,
    package: &Table,
    target: &serde_json::Value,
    kind: &str,
    name: &str,
    root: &Path,
) -> Result<Option<&'a Table>, PhaseFailure> {
    let rows = document
        .get(kind)
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| failure("Cargo executable targets are not an array"))
        })
        .transpose()?;
    let mut selected = None;
    let mut renamed_source = false;
    for row in rows.into_iter().flatten() {
        let table = row
            .as_table()
            .ok_or_else(|| failure("Cargo target declaration is not a table"))?;
        let declared_name =
            string(table, "name")?.ok_or_else(|| failure("explicit Cargo target has no name"))?;
        if declared_name == name {
            if selected.replace(table).is_some() {
                return Err(failure("duplicate declared Cargo target name"));
            }
        } else if let Some(path) = string(table, "path")? {
            // A renamed explicit target suppresses auto-discovery of its source.
            if target["src_path"]
                .as_str()
                .is_some_and(|source| root.join(path) == Path::new(source))
            {
                renamed_source = true;
            }
        }
    }
    if selected.is_none() {
        if renamed_source {
            return Err(failure(
                "Cargo source belongs to a differently named explicit target",
            ));
        }
        let auto = boolean(
            package,
            if kind == "bin" {
                "autobins"
            } else {
                "autotests"
            },
        )?;
        if auto == Some(false)
            || (auto.is_none() && rows.is_some() && package_edition(package, target)? == "2015")
        {
            return Err(failure(
                "Cargo target is absent and autodiscovery is disabled",
            ));
        }
    }
    Ok(selected)
}

fn inferred_source(
    kind: &str,
    name: &str,
    package: &str,
    root: &Path,
    explicit: bool,
    package_table: &Table,
    target: &serde_json::Value,
) -> Result<PathBuf, PhaseFailure> {
    if name.starts_with('.') || name.contains(['/', '\\']) {
        return Err(failure(
            "Cargo target name is not safe for conventional discovery",
        ));
    }
    let mut candidates = if kind == "lib" {
        vec![root.join("src/lib.rs")]
    } else {
        let directory = if kind == "bin" { "src/bin" } else { "tests" };
        let mut paths = vec![root.join(directory).join(format!("{name}.rs"))];
        let nested = root.join(directory).join(name);
        if std::fs::symlink_metadata(&nested).is_ok_and(|metadata| metadata.is_dir()) {
            paths.push(nested.join("main.rs"));
        }
        if kind == "bin" && name == package {
            paths.push(root.join("src/main.rs"));
        }
        paths
    };
    candidates.retain(|path| path.is_file());
    if candidates.is_empty()
        && explicit
        && kind == "lib"
        && package_edition(package_table, target)? == "2015"
    {
        let legacy = root.join("src").join(format!("{name}.rs"));
        if legacy.is_file() {
            candidates.push(legacy);
        }
    }
    match candidates.as_slice() {
        [path] => Ok(path.clone()),
        _ => Err(failure(
            "Cargo target source is missing or has ambiguous conventional paths",
        )),
    }
}

fn package_edition<'a>(
    package: &'a Table,
    target: &'a serde_json::Value,
) -> Result<&'a str, PhaseFailure> {
    match package.get("edition") {
        None => Ok("2015"),
        Some(toml::Value::String(edition))
            if matches!(edition.as_str(), "2015" | "2018" | "2021" | "2024") =>
        {
            Ok(edition)
        }
        Some(toml::Value::Table(table))
            if table.get("workspace") == Some(&toml::Value::Boolean(true)) =>
        {
            target["edition"]
                .as_str()
                .filter(|edition| matches!(*edition, "2015" | "2018" | "2021" | "2024"))
                .ok_or_else(|| {
                    failure("inherited Cargo edition is missing from its observed target")
                })
        }
        _ => Err(failure("Cargo package edition is malformed")),
    }
}

fn string<'a>(table: &'a Table, key: &str) -> Result<Option<&'a str>, PhaseFailure> {
    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| failure(format!("Cargo {key} setting is not a string")))
        })
        .transpose()
}

fn boolean(table: &Table, key: &str) -> Result<Option<bool>, PhaseFailure> {
    table
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| failure(format!("Cargo {key} setting is not Boolean")))
        })
        .transpose()
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "harness-declaration-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_target_declaration_test.rs"]
mod tests;
