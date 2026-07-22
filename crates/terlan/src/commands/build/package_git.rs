use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use super::package_artifact::{
    active_artifact_target, import_artifact, validate_cached_artifact, validate_lock_entry,
    LockedPackageArtifact,
};
use super::project_manifest::{self, ProjectDependencySource, ProjectManifest};
use super::{project_manifest_path, CliCommand, TERLAN_PROJECT_MANIFEST_FILE};

const LOCKFILE_NAME: &str = "terlan.lock";
const LOCKFILE_VERSION: u32 = 1;
const RESOLVER_VERSION: &str = "terlan-0.0.7";

/// Runs explicit package source operations.
pub(super) fn run(cmd: CliCommand) -> ExitCode {
    let args = match parse_fetch_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match fetch_project_dependencies(&args) {
        Ok((package_count, artifact_count, cache_root)) => {
            println!(
                "resolved {package_count} immutable Git package(s) and {artifact_count} target artifact(s) into {}",
                cache_root.display()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Parsed explicit package fetch arguments.
struct PackageFetchArgs {
    /// Project containing the dependency graph.
    project_dir: PathBuf,
    /// Optional platform target override.
    target: Option<String>,
    /// Local artifact archives admitted during this fetch.
    artifacts: Vec<PathBuf>,
}

/// Parses `terlc package fetch` without performing filesystem or network work.
fn parse_fetch_args(args: &[String]) -> Result<PackageFetchArgs, String> {
    if args.first().map(String::as_str) != Some("fetch") {
        return Err(package_fetch_usage());
    }
    let mut project_dir = None;
    let mut target = None;
    let mut artifacts = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("terlc package fetch --target requires a value".into());
                };
                if target.replace(value.clone()).is_some() {
                    return Err("terlc package fetch received duplicate --target".into());
                }
                index += 2;
            }
            "--artifact" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("terlc package fetch --artifact requires a path".into());
                };
                artifacts.push(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(format!("unsupported package fetch option: {value}"));
            }
            value => {
                if project_dir.replace(PathBuf::from(value)).is_some() {
                    return Err(package_fetch_usage());
                }
                index += 1;
            }
        }
    }
    Ok(PackageFetchArgs {
        project_dir: project_dir.unwrap_or_else(|| PathBuf::from(".")),
        target,
        artifacts,
    })
}

/// Returns the stable package fetch usage line.
fn package_fetch_usage() -> String {
    "usage: terlc package fetch [project-dir] [--target <triple>] [--artifact <archive.tar.zst>]..."
        .into()
}

/// Immutable Git package cache and lockfile entries used by normal builds.
pub(super) struct GitDependencyCache {
    project_dir: PathBuf,
    cache_root: PathBuf,
    entries: BTreeMap<(String, String, String), LockedGitPackage>,
    artifacts: BTreeMap<(String, String, String), LockedPackageArtifact>,
    target: String,
}

/// One resolved immutable dependency source and optional packaged runtime.
pub(super) struct ResolvedPackageDependency {
    /// Package directory containing `terlan.toml` and source roots.
    pub(super) package_dir: PathBuf,
    /// Absolute environment bindings provided by a target artifact.
    pub(super) artifact_environment: Vec<(String, PathBuf)>,
}

impl GitDependencyCache {
    /// Loads a checked-in lockfile when one exists.
    pub(super) fn load_if_present(project_dir: &Path) -> Result<Self, String> {
        let lock_path = project_dir.join(LOCKFILE_NAME);
        let lock = if lock_path.is_file() {
            read_lockfile(&lock_path)?
        } else {
            ParsedPackageLockfile::default()
        };
        Ok(Self {
            project_dir: project_dir.to_path_buf(),
            cache_root: package_cache_root(project_dir),
            entries: lock.packages,
            artifacts: lock.artifacts,
            target: active_artifact_target(None)?,
        })
    }

    /// Resolves one Git dependency strictly from the verified cache.
    pub(super) fn resolve(
        &self,
        alias: &str,
        url: &str,
        rev: &str,
    ) -> Result<ResolvedPackageDependency, String> {
        let key = (alias.to_string(), url.to_string(), rev.to_ascii_lowercase());
        let Some(entry) = self.entries.get(&key) else {
            return Err(format!(
                "error[package_git_not_locked]: Git dependency `{alias}` at `{url}` revision `{rev}` is not present in {}; run `terlc package fetch {}`",
                self.project_dir.join(LOCKFILE_NAME).display(),
                self.project_dir.display()
            ));
        };
        let checkout = self.cache_root.join("git").join(&entry.rev);
        validate_cached_checkout(&checkout, entry)?;
        let artifact_key = (
            entry.name.clone(),
            entry.version.clone(),
            self.target.clone(),
        );
        if let Some(artifact) = self.artifacts.get(&artifact_key) {
            let cached = validate_cached_artifact(&self.cache_root, artifact)?;
            return Ok(ResolvedPackageDependency {
                package_dir: cached.package_dir,
                artifact_environment: cached.environment,
            });
        }
        Ok(ResolvedPackageDependency {
            package_dir: checkout,
            artifact_environment: Vec::new(),
        })
    }
}

/// Fetches every Git dependency reachable from one project and writes its lockfile.
#[cfg(test)]
fn fetch_project_git_dependencies(project_dir: &Path) -> Result<usize, String> {
    let args = PackageFetchArgs {
        project_dir: project_dir.to_path_buf(),
        target: None,
        artifacts: Vec::new(),
    };
    fetch_project_dependencies(&args).map(|(count, _, _)| count)
}

/// Fetches Git sources plus explicitly supplied target artifacts and writes one lockfile.
fn fetch_project_dependencies(args: &PackageFetchArgs) -> Result<(usize, usize, PathBuf), String> {
    let project_dir = &args.project_dir;
    let project_dir = project_dir.canonicalize().map_err(|error| {
        format!(
            "terlc package fetch cannot resolve project directory {}: {error}",
            project_dir.display()
        )
    })?;
    let manifest_path = project_manifest_path(&project_dir);
    if !manifest_path.is_file() {
        return Err(format!(
            "terlc package fetch requires {} at {}",
            TERLAN_PROJECT_MANIFEST_FILE,
            manifest_path.display()
        ));
    }
    let manifest = project_manifest::read_project_manifest(&manifest_path)?;
    let mut fetcher = GitPackageFetcher::new(&project_dir);
    fetcher.resolve_package(&project_dir, &manifest)?;
    let target = active_artifact_target(args.target.as_deref())?;
    let existing_lock_path = project_dir.join(LOCKFILE_NAME);
    let mut artifacts = if existing_lock_path.is_file() {
        read_lockfile(&existing_lock_path)?.artifacts
    } else {
        BTreeMap::new()
    };
    artifacts.retain(|_, artifact| {
        fetcher
            .packages
            .values()
            .any(|package| package.name == artifact.package && package.version == artifact.version)
    });
    for artifact in artifacts.values() {
        validate_cached_artifact(&fetcher.cache_root, artifact)?;
    }
    for archive in &args.artifacts {
        let entry = import_artifact(&fetcher.cache_root, archive, &target)?;
        if !fetcher
            .packages
            .values()
            .any(|package| package.name == entry.package && package.version == entry.version)
        {
            return Err(format!(
                "error[package_artifact_unmatched]: artifact `{} {}` does not match a locked Git dependency",
                entry.package, entry.version
            ));
        }
        let key = (
            entry.package.clone(),
            entry.version.clone(),
            entry.target.clone(),
        );
        if artifacts.insert(key, entry).is_some() {
            return Err(
                "error[package_artifact_duplicate]: duplicate package artifact target".into(),
            );
        }
    }
    write_lockfile(
        &project_dir.join(LOCKFILE_NAME),
        &fetcher.packages,
        &artifacts,
    )?;
    Ok((fetcher.packages.len(), artifacts.len(), fetcher.cache_root))
}

struct GitPackageFetcher {
    cache_root: PathBuf,
    visiting: BTreeSet<PathBuf>,
    visited: BTreeSet<PathBuf>,
    packages: BTreeMap<(String, String, String), LockedGitPackage>,
}

impl GitPackageFetcher {
    fn new(project_dir: &Path) -> Self {
        Self {
            cache_root: package_cache_root(project_dir),
            visiting: BTreeSet::new(),
            visited: BTreeSet::new(),
            packages: BTreeMap::new(),
        }
    }

    fn resolve_package(
        &mut self,
        package_dir: &Path,
        manifest: &ProjectManifest,
    ) -> Result<(), String> {
        let package_dir = package_dir.canonicalize().map_err(|error| {
            format!(
                "terlc package fetch cannot canonicalize package {}: {error}",
                package_dir.display()
            )
        })?;
        if self.visited.contains(&package_dir) {
            return Ok(());
        }
        if !self.visiting.insert(package_dir.clone()) {
            return Err(format!(
                "error[package_dependency_cycle]: package dependency cycle includes `{}` at {}",
                manifest.package.name,
                package_dir.display()
            ));
        }

        for dependency in &manifest.dependencies {
            match &dependency.source {
                ProjectDependencySource::Path { path } => {
                    let dependency_dir = package_dir.join(path).canonicalize().map_err(|error| {
                        format!(
                            "terlc package fetch path dependency `{}` cannot be resolved: {} ({error})",
                            dependency.alias,
                            package_dir.join(path).display()
                        )
                    })?;
                    let dependency_manifest = read_dependency_manifest(
                        &dependency.alias,
                        &dependency_dir,
                        "path dependency",
                    )?;
                    self.resolve_package(&dependency_dir, &dependency_manifest)?;
                }
                ProjectDependencySource::Git { url, rev } => {
                    let checkout =
                        self.fetch_git_dependency(&package_dir, &dependency.alias, url, rev)?;
                    let dependency_manifest =
                        read_dependency_manifest(&dependency.alias, &checkout, "Git dependency")?;
                    let tree = git_output(&checkout, &["rev-parse", "HEAD^{tree}"])?;
                    self.packages.insert(
                        (
                            dependency.alias.clone(),
                            url.clone(),
                            rev.to_ascii_lowercase(),
                        ),
                        LockedGitPackage {
                            alias: dependency.alias.clone(),
                            name: dependency_manifest.package.name.clone(),
                            version: dependency_manifest.package.version.clone(),
                            source: "git".to_string(),
                            url: url.clone(),
                            rev: rev.to_ascii_lowercase(),
                            checksum: format!("git-tree:{tree}"),
                            capabilities: dependency_manifest
                                .native_rust
                                .as_ref()
                                .map(|_| vec!["native-process-helper".to_string()])
                                .unwrap_or_default(),
                        },
                    );
                    self.resolve_package(&checkout, &dependency_manifest)?;
                }
                ProjectDependencySource::Npm { .. } | ProjectDependencySource::Cargo { .. } => {}
            }
        }

        self.visiting.remove(&package_dir);
        self.visited.insert(package_dir);
        Ok(())
    }

    fn fetch_git_dependency(
        &self,
        depending_dir: &Path,
        alias: &str,
        url: &str,
        rev: &str,
    ) -> Result<PathBuf, String> {
        let checkout = self.cache_root.join("git").join(rev.to_ascii_lowercase());
        if checkout.exists() {
            validate_checkout_identity(&checkout, url, rev)?;
            return Ok(checkout);
        }

        fs::create_dir_all(self.cache_root.join("git")).map_err(|error| {
            format!(
                "error[package_cache_create_failed]: cannot create package cache {}: {error}",
                self.cache_root.display()
            )
        })?;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temporary = self.cache_root.join("git").join(format!(
            ".{}.tmp-{}-{unique}",
            rev.to_ascii_lowercase(),
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let clone = Command::new("git")
            .args(["clone", "--quiet", "--no-checkout", "--"])
            .arg(url)
            .arg(&temporary)
            .current_dir(depending_dir)
            .output()
            .map_err(|error| {
                format!(
                    "error[package_git_unavailable]: failed to launch git for `{alias}`: {error}"
                )
            })?;
        if !clone.status.success() {
            let _ = fs::remove_dir_all(&temporary);
            return Err(format!(
                "error[package_git_fetch_failed]: failed to clone Git dependency `{alias}` from `{url}`: {}",
                String::from_utf8_lossy(&clone.stderr).trim()
            ));
        }
        let checkout_result = git_status(&temporary, &["checkout", "--quiet", "--detach", rev]);
        if let Err(message) = checkout_result {
            let _ = fs::remove_dir_all(&temporary);
            return Err(format!(
                "error[package_git_revision_missing]: Git dependency `{alias}` does not contain revision `{rev}`: {message}"
            ));
        }
        if let Err(message) = validate_checkout_identity(&temporary, url, rev) {
            let _ = fs::remove_dir_all(&temporary);
            return Err(message);
        }
        match fs::rename(&temporary, &checkout) {
            Ok(()) => {}
            Err(_) if checkout.exists() => {
                let _ = fs::remove_dir_all(&temporary);
                validate_checkout_identity(&checkout, url, rev)?;
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&temporary);
                return Err(format!(
                    "error[package_cache_publish_failed]: cannot publish Git dependency cache {}: {error}",
                    checkout.display()
                ));
            }
        }
        Ok(checkout)
    }
}

fn package_cache_root(project_dir: &Path) -> PathBuf {
    env::var_os("TERLAN_PACKAGE_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| project_dir.join(".terlan/packages"))
}

fn read_dependency_manifest(
    alias: &str,
    package_dir: &Path,
    source: &str,
) -> Result<ProjectManifest, String> {
    let manifest_path = project_manifest_path(package_dir);
    if !manifest_path.is_file() {
        return Err(format!(
            "error[package_manifest_missing]: {source} `{alias}` does not contain {}: {}",
            TERLAN_PROJECT_MANIFEST_FILE,
            manifest_path.display()
        ));
    }
    project_manifest::read_project_manifest(&manifest_path)
}

fn validate_cached_checkout(checkout: &Path, entry: &LockedGitPackage) -> Result<(), String> {
    validate_checkout_identity(checkout, &entry.url, &entry.rev)?;
    let tree = git_output(checkout, &["rev-parse", "HEAD^{tree}"])?;
    let checksum = format!("git-tree:{tree}");
    if checksum != entry.checksum {
        return Err(format!(
            "error[package_cache_checksum_mismatch]: cached package `{}` expected checksum `{}`, found `{checksum}`",
            entry.name, entry.checksum
        ));
    }
    let manifest = read_dependency_manifest(&entry.alias, checkout, "cached Git dependency")?;
    if manifest.package.name != entry.name || manifest.package.version != entry.version {
        return Err(format!(
            "error[package_cache_identity_mismatch]: cached package expected `{} {}`, found `{} {}`",
            entry.name, entry.version, manifest.package.name, manifest.package.version
        ));
    }
    let capabilities = manifest
        .native_rust
        .as_ref()
        .map(|_| vec!["native-process-helper".to_string()])
        .unwrap_or_default();
    if capabilities != entry.capabilities {
        return Err(format!(
            "error[package_cache_capability_mismatch]: cached package `{}` capabilities do not match {}",
            entry.name, LOCKFILE_NAME
        ));
    }
    Ok(())
}

fn validate_checkout_identity(checkout: &Path, url: &str, rev: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(checkout).map_err(|error| {
        format!(
            "error[package_git_not_cached]: cached Git revision `{rev}` is missing at {}: {error}",
            checkout.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "error[package_cache_unsafe_path]: cached Git revision `{rev}` is not a real directory: {}",
            checkout.display()
        ));
    }
    let actual_rev = git_output(checkout, &["rev-parse", "HEAD"])?;
    if !actual_rev.eq_ignore_ascii_case(rev) {
        return Err(format!(
            "error[package_cache_revision_mismatch]: cached Git package at {} expected revision `{rev}`, found `{actual_rev}`",
            checkout.display()
        ));
    }
    let actual_url = git_output(checkout, &["remote", "get-url", "origin"])?;
    if actual_url != url {
        return Err(format!(
            "error[package_cache_provenance_mismatch]: cached Git revision `{rev}` expected source `{url}`, found `{actual_url}`"
        ));
    }
    let dirty = git_output(checkout, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err(format!(
            "error[package_cache_dirty]: cached Git revision `{rev}` contains modified or untracked files"
        ));
    }
    Ok(())
}

fn git_output(repository: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .map_err(|error| format!("failed to launch git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            repository.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_status(repository: &Path, args: &[&str]) -> Result<(), String> {
    git_output(repository, args).map(|_| ())
}

#[derive(Debug, Clone, Deserialize)]
struct PackageLockfile {
    version: u32,
    resolver: String,
    #[serde(default)]
    package: Vec<LockedGitPackage>,
    #[serde(default)]
    artifact: Vec<LockedPackageArtifact>,
}

/// Validated package and artifact maps loaded from one lockfile.
#[derive(Default)]
struct ParsedPackageLockfile {
    /// Immutable Git package entries.
    packages: BTreeMap<(String, String, String), LockedGitPackage>,
    /// Target artifact entries keyed by package, version, and target.
    artifacts: BTreeMap<(String, String, String), LockedPackageArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct LockedGitPackage {
    alias: String,
    name: String,
    version: String,
    source: String,
    url: String,
    rev: String,
    checksum: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

fn read_lockfile(path: &Path) -> Result<ParsedPackageLockfile, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("{}: failed to read lockfile: {error}", path.display()))?;
    let lock: PackageLockfile = basic_toml::from_str(&text)
        .map_err(|error| format!("{}: invalid Terlan lockfile: {error}", path.display()))?;
    if lock.version != LOCKFILE_VERSION || lock.resolver != RESOLVER_VERSION {
        return Err(format!(
            "error[package_lockfile_version_unsupported]: {} requires version {LOCKFILE_VERSION} and resolver `{RESOLVER_VERSION}`",
            path.display()
        ));
    }
    let mut entries = BTreeMap::new();
    for entry in lock.package {
        if entry.source != "git"
            || !matches!(entry.rev.len(), 40 | 64)
            || !entry.rev.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !entry.checksum.starts_with("git-tree:")
        {
            return Err(format!(
                "error[package_lockfile_entry_invalid]: {} contains an invalid Git package entry for `{}`",
                path.display(),
                entry.alias
            ));
        }
        let key = (
            entry.alias.clone(),
            entry.url.clone(),
            entry.rev.to_ascii_lowercase(),
        );
        if entries.insert(key, entry).is_some() {
            return Err(format!(
                "error[package_lockfile_duplicate]: {} contains a duplicate Git source",
                path.display()
            ));
        }
    }
    let mut artifacts = BTreeMap::new();
    for entry in lock.artifact {
        validate_lock_entry(&entry).map_err(|message| format!("{}: {message}", path.display()))?;
        let key = (
            entry.package.clone(),
            entry.version.clone(),
            entry.target.clone(),
        );
        if artifacts.insert(key, entry).is_some() {
            return Err(format!(
                "error[package_lockfile_duplicate]: {} contains a duplicate artifact target",
                path.display()
            ));
        }
    }
    Ok(ParsedPackageLockfile {
        packages: entries,
        artifacts,
    })
}

fn write_lockfile(
    path: &Path,
    packages: &BTreeMap<(String, String, String), LockedGitPackage>,
    artifacts: &BTreeMap<(String, String, String), LockedPackageArtifact>,
) -> Result<(), String> {
    let mut text = format!("version = {LOCKFILE_VERSION}\nresolver = \"{RESOLVER_VERSION}\"\n");
    for package in packages.values() {
        text.push_str("\n[[package]]\n");
        for (name, value) in [
            ("alias", package.alias.as_str()),
            ("name", package.name.as_str()),
            ("version", package.version.as_str()),
            ("source", package.source.as_str()),
            ("url", package.url.as_str()),
            ("rev", package.rev.as_str()),
            ("checksum", package.checksum.as_str()),
        ] {
            text.push_str(&format!("{name} = \"{}\"\n", toml_escape(value)));
        }
        text.push_str("capabilities = [");
        for (index, capability) in package.capabilities.iter().enumerate() {
            if index > 0 {
                text.push_str(", ");
            }
            text.push_str(&format!("\"{}\"", toml_escape(capability)));
        }
        text.push_str("]\n");
    }
    for artifact in artifacts.values() {
        text.push_str("\n[[artifact]]\n");
        for (name, value) in [
            ("package", artifact.package.as_str()),
            ("version", artifact.version.as_str()),
            ("target", artifact.target.as_str()),
            ("schema", artifact.schema.as_str()),
            ("checksum", artifact.checksum.as_str()),
            ("cache_key", artifact.cache_key.as_str()),
            ("terlan_package", artifact.terlan_package.as_str()),
        ] {
            text.push_str(&format!("{name} = \"{}\"\n", toml_escape(value)));
        }
        text.push_str("environment = [");
        for (index, binding) in artifact.environment.iter().enumerate() {
            if index > 0 {
                text.push_str(", ");
            }
            text.push_str(&format!(
                "{{ name = \"{}\", path = \"{}\" }}",
                toml_escape(&binding.name),
                toml_escape(&binding.path)
            ));
        }
        text.push_str("]\n");
    }
    let temporary = path.with_extension(format!("lock.tmp-{}", std::process::id()));
    fs::write(&temporary, text).map_err(|error| {
        format!(
            "error[package_lockfile_write_failed]: cannot write {}: {error}",
            temporary.display()
        )
    })?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!(
            "error[package_lockfile_write_failed]: cannot publish {}: {error}",
            path.display()
        )
    })
}

fn toml_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
#[path = "package_git_test.rs"]
mod tests;
