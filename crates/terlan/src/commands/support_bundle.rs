use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::release_layout::installed_share_root;

const DEFAULT_OUTPUT: &str = "target/support/terlan-support-bundle.json";

#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    target: PathBuf,
    diagnostic: Option<PathBuf>,
    output: PathBuf,
}

#[derive(Debug)]
struct SupportBundleError(String);

impl std::fmt::Display for SupportBundleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SupportBundleError {}

fn failure(message: impl Into<String>) -> SupportBundleError {
    SupportBundleError(message.into())
}

#[derive(Serialize)]
struct FileEvidence {
    kind: &'static str,
    name: String,
    sha256: String,
    size_bytes: u64,
}

/// Generates a deterministic, redacted support bundle for an installed compiler.
///
/// The bundle records hashes and structural metadata, never source text,
/// environment values, credentials, database URLs, or host-local absolute paths.
pub(crate) fn run(args: &[String]) -> ExitCode {
    match parse_arguments(args).and_then(|arguments| write_bundle(&arguments)) {
        Ok(path) => {
            println!("wrote deterministic support bundle to {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[support.bundle]: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_arguments(args: &[String]) -> Result<Arguments, SupportBundleError> {
    if args.first().map(String::as_str) != Some("bundle") {
        return Err(failure("expected `terlc support bundle`"));
    }
    let mut target = None;
    let mut diagnostic = None;
    let mut output = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--diagnostic" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| failure("--diagnostic requires a path"))?;
                diagnostic = Some(PathBuf::from(value));
            }
            "--out" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| failure("--out requires a path"))?;
                output = Some(PathBuf::from(value));
            }
            argument if argument.starts_with('-') => {
                return Err(failure(format!(
                    "unknown support bundle option `{argument}`"
                )));
            }
            argument if target.is_none() => target = Some(PathBuf::from(argument)),
            argument => {
                return Err(failure(format!(
                    "unexpected support bundle argument `{argument}`"
                )))
            }
        }
        index += 1;
    }
    Ok(Arguments {
        target: target.unwrap_or_else(|| PathBuf::from(".")),
        diagnostic,
        output: output.unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT)),
    })
}

fn write_bundle(arguments: &Arguments) -> Result<PathBuf, SupportBundleError> {
    let target = file_evidence(&arguments.target, target_kind(&arguments.target))?;
    let executable = std::env::current_exe()
        .map_err(|error| failure(format!("cannot identify installed compiler: {error}")))?;
    let compiler = file_evidence(&executable, "installed-compiler")?;
    let installed_root = executable.parent().unwrap_or_else(|| Path::new("."));
    let vm = optional_file_evidence(
        &installed_root.join(executable_name("terlan-vm")),
        "installed-vm",
    )?;
    let lsp = optional_file_evidence(
        &installed_root.join(executable_name("terlan-lsp")),
        "installed-lsp",
    )?;
    let manifest_root = if arguments.target.is_dir() {
        arguments.target.as_path()
    } else {
        arguments.target.parent().unwrap_or_else(|| Path::new("."))
    };
    let package_manifest =
        optional_file_evidence(&manifest_root.join("terlan.toml"), "package-manifest")?;
    let package_lock =
        optional_file_evidence(&manifest_root.join("terlan.lock"), "package-resolution")?;
    let share_root = installed_share_root();
    let stdlib_manifest = installed_stdlib_manifest(installed_root, share_root.as_deref())
        .map(|path| file_evidence(&path, "stdlib-manifest"))
        .transpose()?;
    let diagnostic_catalog = share_root
        .as_deref()
        .map(|root| root.join("docs/release/DIAGNOSTIC_CATALOG_0_0_8.json"))
        .map(|path| optional_file_evidence(&path, "diagnostic-catalog"))
        .transpose()?
        .flatten();
    let release_checksums =
        optional_file_evidence(&installed_root.join("SHA256SUMS"), "release-checksums")?;
    let diagnostic = arguments
        .diagnostic
        .as_deref()
        .map(diagnostic_evidence)
        .transpose()?;
    let document = json!({
        "schema": "terlan.support-bundle.v1",
        "version": env!("CARGO_PKG_VERSION"),
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "redaction_policy": {
            "environment_values": "omitted",
            "source_text": "omitted",
            "absolute_paths": "basename-only",
            "secret_fields": "hash-and-shape-only",
        },
        "command": "terlc support bundle <target> --out <bundle>",
        "compiler": compiler,
        "vm": vm,
        "lsp": lsp,
        "target": target,
        "package_manifest": package_manifest,
        "package_resolution": package_lock,
        "stdlib": stdlib_manifest,
        "diagnostic_catalog": diagnostic_catalog,
        "release_checksums": release_checksums,
        "diagnostic": diagnostic,
        "runtime_snapshot": {
            "target_kind": target_kind(&arguments.target),
            "content_captured": false,
        },
        "timing_summary": {
            "source": "release-gate-report",
            "embedded": false,
        },
    });
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| failure(format!("cannot serialize support bundle: {error}")))?;
    if let Some(parent) = arguments.output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| failure(format!("cannot create support bundle directory: {error}")))?;
    }
    fs::write(&arguments.output, bytes)
        .map_err(|error| failure(format!("cannot write support bundle: {error}")))?;
    Ok(arguments.output.clone())
}

fn target_kind(path: &Path) -> &'static str {
    if path.is_dir() {
        "project"
    } else if path.extension().is_some_and(|extension| extension == "tvm") {
        "vm-image"
    } else {
        "file"
    }
}

fn executable_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn installed_stdlib_manifest(installed_root: &Path, share_root: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = share_root.map(|root| root.join("std/manifest.toml")) {
        if path.is_file() {
            return Some(path);
        }
    }
    let installed = installed_root.join("std/manifest.toml");
    if installed.is_file() {
        return Some(installed);
    }
    std::env::var_os("TERLAN_STDLIB_ROOT")
        .map(PathBuf::from)
        .map(|root| root.join("manifest.toml"))
        .filter(|path| path.is_file())
}

fn optional_file_evidence(
    path: &Path,
    kind: &'static str,
) -> Result<Option<FileEvidence>, SupportBundleError> {
    if path.is_file() {
        file_evidence(path, kind).map(Some)
    } else {
        Ok(None)
    }
}

fn file_evidence(path: &Path, kind: &'static str) -> Result<FileEvidence, SupportBundleError> {
    let bytes = if path.is_dir() {
        let manifest = path.join("terlan.toml");
        fs::read(&manifest).map_err(|error| {
            failure(format!(
                "cannot read project manifest `{}`: {error}",
                manifest.display()
            ))
        })?
    } else {
        fs::read(path)
            .map_err(|error| failure(format!("cannot read `{}`: {error}", path.display())))?
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_string();
    Ok(FileEvidence {
        kind,
        name,
        sha256: sha256(&bytes),
        size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    })
}

fn diagnostic_evidence(path: &Path) -> Result<Value, SupportBundleError> {
    let bytes = fs::read(path).map_err(|error| {
        failure(format!(
            "cannot read diagnostic report `{}`: {error}",
            path.display()
        ))
    })?;
    let parsed: Value = serde_json::from_slice(&bytes)
        .map_err(|error| failure(format!("diagnostic report is not valid JSON: {error}")))?;
    let schema = parsed
        .get("schema")
        .and_then(Value::as_str)
        .ok_or_else(|| failure("diagnostic report is missing a string schema"))?;
    Ok(json!({
        "schema": schema,
        "sha256": sha256(&bytes),
        "structured": true,
    }))
}

fn sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
#[path = "support_bundle_test.rs"]
mod tests;
