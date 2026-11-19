//! Content-addressed snapshots for test programs that Cargo may rebuild.

use crate::file_identity::{field, hash_file, hex};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

const MAX_PROGRAM_SET_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PROGRAMS: [(&str, &str); 3] = [
    ("terlc", "TERLAN_RUST_SUITE_TERLC"),
    ("terlan-vm", "TERLAN_RUST_SUITE_VM"),
    ("terlan-native-worker", "TERLAN_RUST_SUITE_NATIVE_WORKER"),
];

struct ProgramIdentity {
    name: &'static str,
    source: PathBuf,
    sha256: String,
    bytes: u64,
    permissions: fs::Permissions,
}

/// Installs the selected programs and prints their immutable shared directory.
pub(super) fn main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let target = match arguments.next() {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            Some(_) => return Err("test-program target directory is empty".into()),
            None => env::var_os("CARGO_TARGET_DIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("target")),
        };
        if arguments.next().is_some() {
            return Err("unexpected test-program snapshot arguments".into());
        }
        let sources = PROGRAMS.map(|(name, variable)| {
            let source = env::var_os(variable)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| target.join("debug").join(executable_name(name)));
            (name, source)
        });
        snapshot(&target, &sources)
    })();
    match result {
        Ok(directory) => {
            println!("{}", directory.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[rust-driver] test-program snapshot: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Routes the complete suite through one immutable set before environment capture.
pub(super) fn route_current() -> Result<PathBuf, String> {
    let target = env::var_os("CARGO_TARGET_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    let sources = PROGRAMS.map(|(name, variable)| {
        let source = env::var_os(variable)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| target.join("debug").join(executable_name(name)));
        (name, source)
    });
    let directory = snapshot(&target, &sources)?;
    route_directory(&directory)?;
    Ok(directory)
}

/// Reinstates the exact immutable routes admitted by a completed suite report.
pub(super) fn route_report(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
        return Err("suite report has an invalid size".into());
    }
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let directory = report_directory(&report)?;
    route_directory(&directory)?;
    Ok(directory)
}

fn report_directory(report: &serde_json::Value) -> Result<PathBuf, String> {
    if report["decision"] != "pass" || report["executable_binding"]["verified"] != true {
        return Err("suite report has no verified executable closeout".into());
    }
    let rows = report["executable_binding"]["before"]
        .as_array()
        .ok_or("suite report has no executable inventory")?;
    let mut directory = None;
    for (name, _) in PROGRAMS {
        let path = rows
            .iter()
            .filter(|row| row["role"] == name)
            .map(|row| row["path"].as_str())
            .collect::<Option<Vec<_>>>()
            .filter(|paths| paths.len() == 1)
            .and_then(|paths| paths.first().copied())
            .map(PathBuf::from)
            .ok_or_else(|| format!("suite report has no unique {name} route"))?;
        if path.file_name() != Some(executable_name(name).as_os_str()) {
            return Err(format!("suite report has an invalid {name} route"));
        }
        let parent = path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("suite report has no {name} snapshot directory"))?;
        if !parent
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with("test-programs-"))
        {
            return Err(format!("suite report has a mutable {name} route"));
        }
        match &directory {
            Some(directory) if directory != &parent => {
                return Err("suite report test programs do not share one snapshot".into())
            }
            None => directory = Some(parent),
            _ => {}
        }
    }
    directory.ok_or_else(|| "suite report has no test-program snapshot".into())
}

fn route_directory(directory: &Path) -> Result<(), String> {
    for (name, variable) in PROGRAMS {
        let path = directory.join(executable_name(name));
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!("{name}: routed snapshot is invalid"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(format!("{name}: routed snapshot is not executable"));
            }
        }
        env::set_var(variable, path);
    }
    Ok(())
}

fn snapshot(target: &Path, sources: &[(&'static str, PathBuf)]) -> Result<PathBuf, String> {
    if sources.len() != PROGRAMS.len() {
        return Err("test-program inventory is incomplete".into());
    }
    let control = ProcessControl::new(Duration::from_secs(120));
    let started = Instant::now();
    let mut remaining = MAX_PROGRAM_SET_BYTES;
    let mut identities = Vec::with_capacity(sources.len());
    for (name, source) in sources {
        if identities
            .iter()
            .any(|identity: &ProgramIdentity| identity.name == *name)
        {
            return Err("test-program inventory contains duplicate names".into());
        }
        let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > remaining {
            return Err(format!("{name}: invalid executable size"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(format!("{name}: executable permission is missing"));
            }
        }
        let mut digest = Sha256::new();
        let bytes = hash_file(source, &mut digest, remaining, control, started)
            .map_err(|error| error.detail)?;
        remaining -= bytes;
        identities.push(ProgramIdentity {
            name,
            source: source.clone(),
            sha256: hex(digest),
            bytes,
            permissions: metadata.permissions(),
        });
    }
    let mut set_digest = Sha256::new();
    field(&mut set_digest, b"terlan.test-program-set.v1");
    for identity in &identities {
        field(&mut set_digest, identity.name.as_bytes());
        field(&mut set_digest, identity.sha256.as_bytes());
        field(&mut set_digest, &identity.bytes.to_le_bytes());
    }
    let directory = target
        .join("validation-tools")
        .join(format!("test-programs-{}", hex(set_digest)));
    create_owned_directory(&directory)?;
    let directory = fs::canonicalize(directory).map_err(|error| error.to_string())?;
    for identity in &identities {
        install(identity, &directory.join(executable_name(identity.name)))?;
    }
    Ok(directory)
}

fn create_owned_directory(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let metadata = fs::symlink_metadata(directory).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("test-program snapshot directory is not an owned directory".into());
    }
    Ok(())
}

fn install(identity: &ProgramIdentity, destination: &Path) -> Result<(), String> {
    if destination.exists() && same_content(identity, destination)? {
        return Ok(());
    }
    let temporary = destination.with_extension(format!("tmp.{}", std::process::id()));
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|error| error.to_string())?;
    }
    fs::copy(&identity.source, &temporary).map_err(|error| error.to_string())?;
    fs::set_permissions(&temporary, identity.permissions.clone())
        .map_err(|error| error.to_string())?;
    if !same_content(identity, &temporary)? {
        let _ = fs::remove_file(&temporary);
        return Err(format!("{}: snapshot copy changed", identity.name));
    }
    if destination.exists() {
        fs::remove_file(destination).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, destination).map_err(|error| error.to_string())
}

fn same_content(identity: &ProgramIdentity, path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() != identity.bytes {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Ok(false);
        }
    }
    let mut digest = Sha256::new();
    hash_file(
        path,
        &mut digest,
        identity.bytes,
        ProcessControl::new(Duration::from_secs(120)),
        Instant::now(),
    )
    .map_err(|error| error.detail)?;
    Ok(hex(digest) == identity.sha256)
}

fn executable_name(name: &str) -> OsString {
    let mut value = OsString::from(name);
    value.push(env::consts::EXE_SUFFIX);
    value
}

#[cfg(test)]
#[path = "test_program_snapshot_test.rs"]
mod tests;
