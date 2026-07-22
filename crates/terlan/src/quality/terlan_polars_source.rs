use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::terlan_quality::QualityResult;

const PACKAGE_DIR_ENV: &str = "TERLAN_POLARS_DIR";
const PACKAGE_SOURCE_ENV: &str = "TERLAN_POLARS_SOURCE";
const PACKAGE_REV_ENV: &str = "TERLAN_POLARS_REV";
const PACKAGE_CACHE_ENV: &str = "TERLAN_POLARS_CACHE_DIR";

/// Inputs used to resolve the external `terlan-polars` package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TerlanPolarsSourceConfig {
    /// Explicit working-tree override, used before every inferred source.
    pub explicit_dir: Option<PathBuf>,
    /// Conventional sibling checkout beside the compiler repository.
    pub sibling_dir: PathBuf,
    /// Git source used only when the requested revision is absent from cache.
    pub source: Option<OsString>,
    /// Full immutable Git revision for nonlocal resolution.
    pub revision: Option<String>,
    /// Root containing revision-addressed verified package checkouts.
    pub cache_root: PathBuf,
}

impl TerlanPolarsSourceConfig {
    /// Reads package source policy from the process environment.
    fn from_environment(root: &Path) -> Self {
        let resolved_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let sibling_dir = resolved_root
            .parent()
            .unwrap_or(resolved_root.as_path())
            .join("terlan-polars");
        let cache_root = nonempty_env(PACKAGE_CACHE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/package-cache/terlan-polars"));
        Self {
            explicit_dir: nonempty_env(PACKAGE_DIR_ENV).map(PathBuf::from),
            sibling_dir,
            source: nonempty_env(PACKAGE_SOURCE_ENV),
            revision: nonempty_env(PACKAGE_REV_ENV)
                .map(|revision| revision.to_string_lossy().into_owned()),
            cache_root,
        }
    }
}

/// Resolves `terlan-polars` without requiring publication from this workspace.
pub(super) fn resolve_terlan_polars_source(root: &Path) -> QualityResult<PathBuf> {
    resolve_source(&TerlanPolarsSourceConfig::from_environment(root))
}

/// Applies deterministic source precedence and verifies immutable cache state.
pub(super) fn resolve_source(config: &TerlanPolarsSourceConfig) -> QualityResult<PathBuf> {
    if let Some(explicit_dir) = &config.explicit_dir {
        return require_package_dir(explicit_dir, PACKAGE_DIR_ENV);
    }
    if config.sibling_dir.is_dir() {
        return Ok(config.sibling_dir.clone());
    }

    let revision = require_revision(config.revision.as_deref())?;
    let cache_entry = config.cache_root.join(revision);
    if cache_entry.exists() {
        verify_cached_checkout(&cache_entry, revision)?;
        return Ok(cache_entry);
    }

    let source = config.source.as_deref().ok_or_else(|| {
        format!(
            "error[terlan_polars_source_unavailable]: no local package or cached revision `{revision}`; set {PACKAGE_DIR_ENV}, prepopulate {}, or set {PACKAGE_SOURCE_ENV}",
            cache_entry.display()
        )
    })?;
    materialize_checkout(source, revision, &config.cache_root, &cache_entry)?;
    Ok(cache_entry)
}

fn nonempty_env(name: &str) -> Option<OsString> {
    env::var_os(name).filter(|value| !value.is_empty())
}

fn require_package_dir(path: &Path, source: &str) -> QualityResult<PathBuf> {
    if path.is_dir() {
        Ok(path.to_path_buf())
    } else {
        Err(format!(
            "error[terlan_polars_package_missing]: {source} points to missing package directory {}",
            path.display()
        ))
    }
}

fn require_revision(revision: Option<&str>) -> QualityResult<&str> {
    let revision = revision.ok_or_else(|| {
        format!(
            "error[terlan_polars_revision_required]: {PACKAGE_REV_ENV} must be a full 40-character Git revision when no local package exists"
        )
    })?;
    if revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(revision)
    } else {
        Err(format!(
            "error[terlan_polars_revision_invalid]: {PACKAGE_REV_ENV} must be a full 40-character Git revision, got `{revision}`"
        ))
    }
}

fn verify_cached_checkout(path: &Path, revision: &str) -> QualityResult<()> {
    if !path.is_dir() {
        return Err(format!(
            "error[terlan_polars_cache_invalid]: cache entry is not a directory: {}",
            path.display()
        ));
    }
    let head = git_stdout(path, ["rev-parse", "HEAD"], "terlan_polars_cache_invalid")?;
    if !head.eq_ignore_ascii_case(revision) {
        return Err(format!(
            "error[terlan_polars_cache_revision_mismatch]: cache entry {} has HEAD `{head}`, expected `{revision}`",
            path.display()
        ));
    }
    let status = git_stdout(
        path,
        ["status", "--porcelain", "--untracked-files=all"],
        "terlan_polars_cache_invalid",
    )?;
    if !status.is_empty() {
        return Err(format!(
            "error[terlan_polars_cache_dirty]: cache entry {} contains modified or untracked files",
            path.display()
        ));
    }
    Ok(())
}

fn materialize_checkout(
    source: &OsStr,
    revision: &str,
    cache_root: &Path,
    cache_entry: &Path,
) -> QualityResult<()> {
    fs::create_dir_all(cache_root).map_err(|error| {
        format!(
            "error[terlan_polars_cache_create]: failed to create {}: {error}",
            cache_root.display()
        )
    })?;
    let staging = cache_root.join(format!(".{revision}.tmp.{}", std::process::id()));
    if staging.exists() {
        return Err(format!(
            "error[terlan_polars_cache_staging_exists]: refusing to overwrite staging path {}",
            staging.display()
        ));
    }

    let clone_output = Command::new("git")
        .args([
            OsStr::new("clone"),
            OsStr::new("--quiet"),
            OsStr::new("--no-checkout"),
        ])
        .arg(source)
        .arg(&staging)
        .output()
        .map_err(|error| {
            format!("error[terlan_polars_source_git]: failed to execute git clone: {error}")
        })?;
    if !clone_output.status.success() {
        cleanup_staging(&staging);
        return Err(command_failure(
            "terlan_polars_source_clone",
            "git clone",
            &clone_output,
        ));
    }

    let checkout_output = Command::new("git")
        .arg("-C")
        .arg(&staging)
        .args(["checkout", "--quiet", "--detach", revision])
        .output()
        .map_err(|error| {
            cleanup_staging(&staging);
            format!("error[terlan_polars_source_git]: failed to execute git checkout: {error}")
        })?;
    if !checkout_output.status.success() {
        cleanup_staging(&staging);
        return Err(command_failure(
            "terlan_polars_source_revision_missing",
            "git checkout",
            &checkout_output,
        ));
    }
    if let Err(error) = verify_cached_checkout(&staging, revision) {
        cleanup_staging(&staging);
        return Err(error);
    }

    match fs::rename(&staging, cache_entry) {
        Ok(()) => verify_cached_checkout(cache_entry, revision),
        Err(_error) if cache_entry.exists() => {
            cleanup_staging(&staging);
            verify_cached_checkout(cache_entry, revision)
        }
        Err(error) => {
            cleanup_staging(&staging);
            Err(format!(
                "error[terlan_polars_cache_publish]: failed to move {} to {}: {error}",
                staging.display(),
                cache_entry.display()
            ))
        }
    }
}

fn git_stdout<const N: usize>(
    directory: &Path,
    args: [&str; N],
    code: &str,
) -> QualityResult<String> {
    let label = format!("git {}", args.join(" "));
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .map_err(|error| format!("error[{code}]: failed to execute {label}: {error}"))?;
    if !output.status.success() {
        return Err(command_failure(code, &label, &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn command_failure(code: &str, command: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!(
        "error[{code}]: {command} failed with status {}: {}",
        output.status,
        stderr.trim()
    )
}

fn cleanup_staging(path: &Path) {
    if path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with('.') && name.contains(".tmp."))
    {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
#[path = "terlan_polars_source_test.rs"]
mod terlan_polars_source_test;
