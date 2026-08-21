//! Trust-before-use whole-graph Terlan Registry resolution.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};

use crate::package_registry::admission::{canonical_package_name, validate_publish_request};
use crate::package_registry::model::{
    DependencySource, Digest, PackageIndexVersion, PackageVersionRecord, PublishRequest,
    SnapshotRecord, MAX_ARCHIVE_BYTES,
};

use super::package_registry_error::RegistryResult;
use super::package_registry_solver::{
    solve_graph, GraphCandidate, GraphDependency, GraphRequirement,
};
use super::package_registry_transport::{atomic_json, sha256_hex, Download, RepositoryClient};
use super::package_registry_trust::{
    state_after_root, state_after_snapshot, verify_package_index, verify_root, verify_snapshot,
    TrustPin, TrustState, VerifiedResource,
};

const LOCKFILE_VERSION: u32 = 3;
const RESOLVER_VERSION: &str = "terlan-registry-resolver-v2";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;

pub(super) fn run(args: &[String], output_root: &Path) -> ExitCode {
    let args = match parse_args(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match resolve_registry_package(&args, output_root) {
        Ok(resolution) => {
            println!(
                "resolved {}@{} and {} package(s) from snapshot sha256:{}",
                resolution.package,
                resolution.version,
                resolution.package_count,
                resolution.snapshot_sha256
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

pub(super) fn run_update(args: &[String], output_root: &Path) -> ExitCode {
    if args.first().map(String::as_str) != Some("update") {
        eprintln!("usage: terlc package update [package]... --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>");
        return ExitCode::from(2);
    }
    let split = args
        .iter()
        .position(|argument| argument.starts_with("--"))
        .unwrap_or(args.len());
    let packages = &args[1..split];
    if packages
        .iter()
        .any(|package| !canonical_package_name(package))
    {
        eprintln!("error[registry_update_package]: update package name is invalid");
        return ExitCode::from(2);
    }
    let mut translated = vec!["resolve".to_string()];
    translated.extend_from_slice(&args[split..]);
    if packages.is_empty() {
        translated.push("--update-all".into());
    } else {
        for package in packages {
            translated.push("--update".into());
            translated.push(package.clone());
        }
    }
    run(&translated, output_root)
}

pub(super) fn run_tree(args: &[String], output_root: &Path) -> ExitCode {
    if args != ["tree"] {
        eprintln!("usage: terlc package tree --out-dir <project-dir>");
        return ExitCode::from(2);
    }
    let lock = match read_lockfile(&output_root.join("terlan.lock")) {
        Ok(lock) => lock,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    for entry in &lock.registry {
        println!("{}@{}", entry.name, entry.version);
        for dependency in &entry.dependencies {
            println!("  -> {dependency}");
        }
    }
    ExitCode::SUCCESS
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolveArgs {
    registry: String,
    trust_root: PathBuf,
    package: Option<String>,
    version: Option<String>,
    updates: BTreeSet<String>,
    update_all: bool,
    allow_yanked: bool,
    offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryResolution {
    package: String,
    version: String,
    package_count: usize,
    snapshot_sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RegistryLockfile {
    pub(super) version: u32,
    pub(super) resolver: String,
    #[serde(default)]
    pub(super) registry: Vec<LockedRegistryPackage>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct LockedRegistryPackage {
    pub(super) alias: String,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) registry: String,
    pub(super) snapshot_sha256: String,
    pub(super) source_identity: String,
    pub(super) archive_sha256: String,
    pub(super) metadata_sha256: String,
    pub(super) cache_key: String,
    pub(super) targets: Vec<String>,
    pub(super) capabilities: Vec<String>,
    #[serde(default)]
    pub(super) dependencies: Vec<String>,
    pub(super) resolver: String,
}

#[derive(Clone)]
struct CandidateMaterial {
    index: PackageIndexVersion,
    metadata: PackageVersionRecord,
    metadata_bytes: Vec<u8>,
}

struct TrustedRepository {
    origin: String,
    client: RepositoryClient,
    trust_state: TrustState,
    snapshot: VerifiedResource<SnapshotRecord>,
    materials: BTreeMap<(String, String), CandidateMaterial>,
}

impl TrustedRepository {
    fn open(args: &ResolveArgs, output_root: &Path) -> RegistryResult<Self> {
        let origin = super::package_publish_live::registry_origin(&args.registry)?;
        let pin: TrustPin =
            serde_json::from_slice(&fs::read(&args.trust_root).map_err(|error| {
                format!(
                    "error[registry_trust_pin_read]: cannot read {}: {error}",
                    args.trust_root.display()
                )
            })?)
            .map_err(|error| format!("error[registry_trust_pin_invalid]: {error}"))?;
        let remote_root = output_root
            .join(".terlan/registry/remotes")
            .join(sha256_hex(origin.as_bytes()));
        let state_path = remote_root.join("trust-state.json");
        let previous_state = read_optional_json::<TrustState>(&state_path)?;
        let client =
            RepositoryClient::new(origin.clone(), remote_root.join("cache"), args.offline)?;

        let root_route = "/repo/v1/root.json";
        let root_download = client.get(root_route, MAX_METADATA_BYTES)?;
        let root = verify_root(&root_download.bytes, &origin, &pin, previous_state.as_ref())?;
        client.commit_verified(root_route, &root_download)?;
        let root_state = state_after_root(&origin, &root, previous_state.as_ref());

        let snapshot_route = "/repo/v1/snapshot.json";
        let snapshot_download = client.get(snapshot_route, MAX_METADATA_BYTES)?;
        let snapshot = verify_snapshot(&snapshot_download.bytes, &origin, &root_state)?;
        client.commit_verified(snapshot_route, &snapshot_download)?;
        let trust_state = state_after_snapshot(&root_state, &snapshot);
        atomic_json(&state_path, &trust_state)?;
        Ok(Self {
            origin,
            client,
            trust_state,
            snapshot,
            materials: BTreeMap::new(),
        })
    }

    fn load_candidates(&mut self, package: &str) -> RegistryResult<Vec<GraphCandidate>> {
        if !canonical_package_name(package) {
            return Err(format!(
                "error[registry_package_name]: dependency `{package}` is not canonical"
            )
            .into());
        }
        let snapshot_package = self
            .snapshot
            .value
            .packages
            .iter()
            .find(|candidate| candidate.name == package)
            .ok_or_else(|| {
                format!(
                    "error[registry_package_missing]: `{package}` is absent from trusted snapshot"
                )
            })?;
        let index_route = format!("/repo/v1/packages/{package}.json");
        let index_download = self.client.get(&index_route, MAX_METADATA_BYTES)?;
        let index = verify_package_index(
            &index_download.bytes,
            &self.origin,
            &index_route,
            package,
            &snapshot_package.index,
            &self.trust_state,
        )?;
        self.client.commit_verified(&index_route, &index_download)?;

        let mut candidates = Vec::new();
        for version in &index.value.versions {
            crate::package_registry::canonical_version(&version.version)
                .map_err(|error| error.to_string())?;
            if !crate::package_registry::requirement_matches(
                &version.requires_terlan,
                env!("CARGO_PKG_VERSION"),
            )
            .unwrap_or(false)
            {
                continue;
            }
            let material = self.load_material(package, &index.value.repository_url, version)?;
            let dependencies = material
                .metadata
                .dependencies
                .iter()
                .filter(|dependency| dependency.source == DependencySource::TerlanRegistry)
                .map(|dependency| GraphDependency {
                    package: dependency.name.clone(),
                    requirement: dependency.requirement.clone(),
                    optional: dependency.optional,
                })
                .collect();
            candidates.push(GraphCandidate {
                package: package.into(),
                version: version.version.clone(),
                yanked: version.yanked,
                dependencies,
            });
            self.materials
                .insert((package.into(), version.version.clone()), material);
        }
        Ok(candidates)
    }

    fn load_material(
        &self,
        package: &str,
        repository_url: &str,
        version: &PackageIndexVersion,
    ) -> RegistryResult<CandidateMaterial> {
        let route = format!(
            "/repo/v1/packages/{}/{}/metadata.json",
            package, version.version
        );
        let download = self.client.get(&route, MAX_METADATA_BYTES)?;
        verify_model_digest_bytes("package metadata", &version.metadata, &download.bytes)?;
        let request: PublishRequest = serde_json::from_slice(&download.bytes)
            .map_err(|error| format!("error[registry_metadata_invalid]: {error}"))?;
        validate_publish_request(&request).map_err(|error| error.to_string())?;
        let metadata = request.package_version;
        if metadata.package.name != package
            || metadata.package.version != version.version
            || metadata.repository_url != repository_url
            || metadata.built_with != version.built_with
            || metadata.requires_terlan != version.requires_terlan
            || metadata.archive.digest != version.archive
            || metadata.documentation.as_ref().map(|value| &value.digest)
                != version.documentation.as_ref()
        {
            return Err(format!(
                "error[registry_metadata_identity_mismatch]: `{package}@{}` differs from its trusted index",
                version.version
            )
            .into());
        }
        for dependency in &metadata.dependencies {
            if dependency.source == DependencySource::TerlanRegistry {
                let dependency_origin =
                    super::package_publish_live::registry_origin(&dependency.registry)?;
                if dependency_origin != self.origin {
                    return Err(format!(
                        "error[registry_dependency_origin]: `{package}@{}` changes Registry origin for `{}`",
                        version.version, dependency.name
                    )
                    .into());
                }
            }
        }
        self.client.commit_verified(&route, &download)?;
        Ok(CandidateMaterial {
            index: version.clone(),
            metadata,
            metadata_bytes: download.bytes,
        })
    }

    fn material(&self, package: &str, version: &str) -> RegistryResult<CandidateMaterial> {
        Ok(self
            .materials
            .get(&(package.into(), version.into()))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "error[registry_solver_internal]: selected `{package}@{version}` has no verified metadata"
                )
            })?)
    }

    fn download_archive(&self, material: &CandidateMaterial) -> RegistryResult<(String, Download)> {
        let identity = &material.metadata.package;
        let route = format!(
            "/repo/v1/packages/{}/{}/archive.tar.zst",
            identity.name, identity.version
        );
        let download = self.client.get(
            &route,
            material
                .metadata
                .archive
                .compressed_bytes
                .min(MAX_ARCHIVE_BYTES),
        )?;
        verify_model_digest_bytes("package archive", &material.index.archive, &download.bytes)?;
        verify_model_digest_bytes(
            "package metadata archive",
            &material.metadata.archive.digest,
            &download.bytes,
        )?;
        if download.bytes.len() as u64 != material.metadata.archive.compressed_bytes {
            return Err(
                "error[registry_archive_size]: package archive byte count differs from metadata"
                    .into(),
            );
        }
        self.client.commit_verified(&route, &download)?;
        Ok((sha256_hex(&download.bytes), download))
    }
}

pub(super) fn resolve_locked_dependency(
    project_root: &Path,
    alias: &str,
    registry: &str,
    requirement: &str,
) -> RegistryResult<PathBuf> {
    let lock_path = project_root.join("terlan.lock");
    if !lock_path.is_file() {
        return Err(format!(
            "error[registry_package_not_locked]: Registry dependency `{alias}` requires `terlc package resolve`"
        )
        .into());
    }
    let origin = super::package_publish_live::registry_origin(registry)?;
    let lock = read_lockfile(&lock_path)?;
    let entry = lock
        .registry
        .iter()
        .find(|entry| {
            entry.alias == alias
                && entry.registry == origin
                && crate::package_registry::requirement_matches(requirement, &entry.version)
                    .unwrap_or(false)
        })
        .ok_or_else(|| {
            format!(
                "error[registry_package_not_locked]: `{alias}` requirement `{requirement}` from `{origin}` is absent from terlan.lock"
            )
        })?;
    let cache_root = project_root
        .join(".terlan/packages/registry")
        .join(&entry.cache_key);
    let archive = cache_root.join("archive.tar.zst");
    verify_digest(
        "locked Registry cache",
        &entry.archive_sha256,
        &hash_file(&archive)?,
    )?;
    let source = cache_root.join("source");
    let manifest = super::project_manifest::read_project_manifest(
        &source.join(super::TERLAN_PROJECT_MANIFEST_FILE),
    )?;
    if manifest.package.name != entry.name || manifest.package.version != entry.version {
        return Err(format!(
            "error[registry_cache_identity_mismatch]: cached package expected `{}@{}`, found `{}@{}`",
            entry.name, entry.version, manifest.package.name, manifest.package.version
        )
        .into());
    }
    Ok(source)
}

fn parse_args(args: &[String]) -> RegistryResult<ResolveArgs> {
    if args.first().map(String::as_str) != Some("resolve") {
        return Err(usage().into());
    }
    let mut registry = None;
    let mut trust_root = None;
    let mut package = None;
    let mut version = None;
    let mut allow_yanked = false;
    let mut offline = false;
    let mut updates = BTreeSet::new();
    let mut update_all = false;
    let mut index = 1;
    while index < args.len() {
        let (slot, label) = match args[index].as_str() {
            "--registry" => (&mut registry, "--registry"),
            "--trust-root" => (&mut trust_root, "--trust-root"),
            "--package" => (&mut package, "--package"),
            "--version" => (&mut version, "--version"),
            "--update" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("terlc package resolve --update requires a package name".into());
                };
                if !canonical_package_name(value) || !updates.insert(value.clone()) {
                    return Err(format!("invalid or duplicate --update package: {value}").into());
                }
                index += 2;
                continue;
            }
            "--update-all" => {
                if update_all {
                    return Err("duplicate --update-all".into());
                }
                update_all = true;
                index += 1;
                continue;
            }
            "--allow-yanked" => {
                if allow_yanked {
                    return Err("duplicate --allow-yanked".into());
                }
                allow_yanked = true;
                index += 1;
                continue;
            }
            "--offline" => {
                if offline {
                    return Err("duplicate --offline".into());
                }
                offline = true;
                index += 1;
                continue;
            }
            option => return Err(format!("unsupported package resolve option: {option}").into()),
        };
        let Some(value) = args.get(index + 1) else {
            return Err(format!("terlc package resolve {label} requires a value").into());
        };
        if slot.replace(value.clone()).is_some() {
            return Err(format!("duplicate {label}").into());
        }
        index += 2;
    }
    if package.is_some() != version.is_some() || (update_all && !updates.is_empty()) {
        return Err(usage().into());
    }
    Ok(ResolveArgs {
        registry: registry.ok_or_else(usage)?,
        trust_root: PathBuf::from(trust_root.ok_or_else(usage)?),
        package,
        version,
        updates,
        update_all,
        allow_yanked,
        offline,
    })
}

fn usage() -> String {
    "usage: terlc package resolve --registry <url> --trust-root <pin.json> [--package <name> --version <version>] [--update <name>]... [--update-all] [--allow-yanked] [--offline] --out-dir <project-dir>".into()
}

fn resolve_registry_package(
    args: &ResolveArgs,
    output_root: &Path,
) -> RegistryResult<RegistryResolution> {
    let mut repository = TrustedRepository::open(args, output_root)?;
    let lock_path = output_root.join("terlan.lock");
    let existing = read_optional_lockfile(&lock_path)?;
    let locked = existing
        .as_ref()
        .map(|lock| {
            lock.registry
                .iter()
                .filter(|entry| entry.registry == repository.origin)
                .map(|entry| (entry.name.clone(), entry.version.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let roots = resolution_roots(args, output_root, &repository.origin)?;
    let display_package = roots
        .first()
        .map(|root| root.package.clone())
        .unwrap_or_else(|| "project".into());
    let updates = if args.update_all {
        locked.keys().cloned().collect()
    } else {
        args.updates.clone()
    };
    let selected = solve_graph(roots, &locked, &updates, args.allow_yanked, |package| {
        repository.load_candidates(package)
    })?;

    let mut entries = Vec::new();
    for (package, candidate) in &selected {
        let material = repository.material(package, &candidate.version)?;
        if let Some(dependency) = material.metadata.dependencies.iter().find(|dependency| {
            !dependency.optional && dependency.source != DependencySource::TerlanRegistry
        }) {
            return Err(format!(
                "error[registry_foreign_dependency_unresolved]: `{}@{}` requires `{}` from {:?}; its verified target adapter is not available",
                package, candidate.version, dependency.name, dependency.source
            )
            .into());
        }
        let (archive_sha256, archive) = repository.download_archive(&material)?;
        cache_archive(output_root, &archive_sha256, &archive)?;
        if let Some(previous) = existing.as_ref().and_then(|lock| {
            lock.registry
                .iter()
                .find(|entry| entry.name == *package && entry.version == candidate.version)
        }) {
            verify_digest(
                "locked package archive",
                &previous.archive_sha256,
                &archive_sha256,
            )?;
            verify_digest(
                "locked package metadata",
                &previous.metadata_sha256,
                &sha256_hex(&material.metadata_bytes),
            )?;
        }
        let dependencies = material
            .metadata
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.source == DependencySource::TerlanRegistry
                    && selected.contains_key(&dependency.name)
            })
            .map(|dependency| format!("{}@{}", dependency.name, selected[&dependency.name].version))
            .collect();
        entries.push(LockedRegistryPackage {
            alias: package.clone(),
            name: package.clone(),
            version: candidate.version.clone(),
            registry: repository.origin.clone(),
            snapshot_sha256: repository.snapshot.envelope_sha256.clone(),
            source_identity: format!("registry:{}/{}", repository.origin, package),
            archive_sha256: archive_sha256.clone(),
            metadata_sha256: sha256_hex(&material.metadata_bytes),
            cache_key: archive_sha256,
            targets: material.metadata.targets,
            capabilities: material.metadata.capabilities,
            dependencies,
            resolver: RESOLVER_VERSION.into(),
        });
    }
    entries.sort_by(|left, right| (&left.alias, &left.name).cmp(&(&right.alias, &right.name)));
    fs::create_dir_all(output_root).map_err(|error| {
        format!(
            "cannot create lockfile directory {}: {error}",
            output_root.display()
        )
    })?;
    write_lockfile(
        &lock_path,
        &RegistryLockfile {
            version: LOCKFILE_VERSION,
            resolver: RESOLVER_VERSION.into(),
            registry: entries,
        },
    )?;
    let selected_names = selected.keys().cloned().collect::<BTreeSet<_>>();
    if !updates.is_subset(&selected_names) {
        let missing = updates
            .difference(&selected_names)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "error[registry_update_missing]: update package(s) are outside the resolved graph: {missing}"
        )
        .into());
    }
    let display_version = selected
        .get(&display_package)
        .map(|candidate| candidate.version.clone())
        .unwrap_or_else(|| "none".into());
    Ok(RegistryResolution {
        package: display_package,
        version: display_version,
        package_count: selected.len(),
        snapshot_sha256: repository.snapshot.envelope_sha256,
    })
}

fn resolution_roots(
    args: &ResolveArgs,
    output_root: &Path,
    origin: &str,
) -> RegistryResult<Vec<GraphRequirement>> {
    if let (Some(package), Some(version)) = (&args.package, &args.version) {
        if !canonical_package_name(package) {
            return Err("error[registry_package_name]: package name is not canonical".into());
        }
        crate::package_registry::canonical_version(version).map_err(|error| error.to_string())?;
        return Ok(vec![GraphRequirement {
            package: package.clone(),
            requirement: format!("={version}"),
            requested_by: "command-line".into(),
        }]);
    }
    let manifest_path = output_root.join(super::TERLAN_PROJECT_MANIFEST_FILE);
    let manifest = super::project_manifest::read_project_manifest(&manifest_path)?;
    let mut roots = Vec::new();
    for dependency in &manifest.dependencies {
        if let super::project_manifest::ProjectDependencySource::Registry { registry, version } =
            &dependency.source
        {
            let dependency_origin = super::package_publish_live::registry_origin(registry)?;
            if dependency_origin != origin {
                return Err(format!(
                    "error[registry_dependency_origin]: project dependency `{}` uses `{dependency_origin}`, expected `{origin}`",
                    dependency.alias
                )
                .into());
            }
            crate::package_registry::parse_requirement(version)
                .map_err(|error| error.to_string())?;
            roots.push(GraphRequirement {
                package: dependency.alias.clone(),
                requirement: version.clone(),
                requested_by: manifest.package.name.clone(),
            });
        }
    }
    roots.sort_by(|left, right| left.package.cmp(&right.package));
    if roots.is_empty() {
        return Err(
            "error[registry_project_empty]: project has no Registry dependencies to resolve".into(),
        );
    }
    Ok(roots)
}

fn read_optional_json<T: for<'de> Deserialize<'de>>(path: &Path) -> RegistryResult<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("error[registry_trust_state]: {error}"))?,
    )
    .map(Some)
    .map_err(|error| format!("error[registry_trust_state]: {error}"))?)
}

fn read_optional_lockfile(path: &Path) -> RegistryResult<Option<RegistryLockfile>> {
    path.is_file().then(|| read_lockfile(path)).transpose()
}

pub(super) fn read_lockfile(path: &Path) -> RegistryResult<RegistryLockfile> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let lock: RegistryLockfile = basic_toml::from_str(&text)
        .map_err(|error| format!("error[registry_lock_invalid]: {}: {error}", path.display()))?;
    let supported = (lock.version == 2 && lock.resolver == "terlan-registry-resolver-v1")
        || (lock.version == LOCKFILE_VERSION && lock.resolver == RESOLVER_VERSION);
    if !supported || (lock.version == 2 && lock.registry.is_empty()) {
        return Err("error[registry_lock_invalid]: unsupported Registry lockfile identity".into());
    }
    Ok(lock)
}

fn write_lockfile(path: &Path, lock: &RegistryLockfile) -> RegistryResult<()> {
    let mut text = format!(
        "version = {}\nresolver = \"{}\"\n",
        lock.version, lock.resolver
    );
    for entry in &lock.registry {
        text.push_str("\n[[registry]]\n");
        for (name, value) in [
            ("alias", entry.alias.as_str()),
            ("name", entry.name.as_str()),
            ("version", entry.version.as_str()),
            ("registry", entry.registry.as_str()),
            ("snapshot_sha256", entry.snapshot_sha256.as_str()),
            ("source_identity", entry.source_identity.as_str()),
            ("archive_sha256", entry.archive_sha256.as_str()),
            ("metadata_sha256", entry.metadata_sha256.as_str()),
            ("cache_key", entry.cache_key.as_str()),
            ("resolver", entry.resolver.as_str()),
        ] {
            text.push_str(&format!("{name} = \"{}\"\n", escape(value)));
        }
        write_array(&mut text, "targets", &entry.targets);
        write_array(&mut text, "capabilities", &entry.capabilities);
        write_array(&mut text, "dependencies", &entry.dependencies);
    }
    Ok(fs::write(path, text)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?)
}

pub(super) fn write_empty_lock(output_root: &Path) -> RegistryResult<()> {
    write_lockfile(
        &output_root.join("terlan.lock"),
        &RegistryLockfile {
            version: LOCKFILE_VERSION,
            resolver: RESOLVER_VERSION.into(),
            registry: Vec::new(),
        },
    )
}

fn write_array(output: &mut String, name: &str, values: &[String]) {
    output.push_str(&format!("{name} = ["));
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&format!("\"{}\"", escape(value)));
    }
    output.push_str("]\n");
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn verify_digest(label: &str, expected: &str, actual: &str) -> RegistryResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(format!("error[registry_checksum_mismatch]: {label} expected sha256:{expected}, found sha256:{actual}").into())
    }
}

fn verify_model_digest_bytes(label: &str, expected: &Digest, bytes: &[u8]) -> RegistryResult<()> {
    if expected.algorithm != "sha256" {
        return Err(format!(
            "error[registry_digest_algorithm_unsupported]: {label} uses `{}`",
            expected.algorithm
        )
        .into());
    }
    verify_digest(label, &expected.value, &sha256_hex(bytes))
}

fn hash_file(path: &Path) -> RegistryResult<String> {
    Ok(fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| {
            format!(
                "error[registry_cache_read]: cannot read {}: {error}",
                path.display()
            )
        })?)
}

fn cache_archive(
    output_root: &Path,
    archive_sha256: &str,
    download: &Download,
) -> RegistryResult<()> {
    let cache_root = output_root
        .join(".terlan/packages/registry")
        .join(archive_sha256);
    let cached_archive = cache_root.join("archive.tar.zst");
    let source = cache_root.join("source");
    if cached_archive.is_file() && source.is_dir() {
        return verify_digest(
            "cached package archive",
            archive_sha256,
            &hash_file(&cached_archive)?,
        );
    }
    if cache_root.exists() {
        return Err(format!(
            "error[registry_cache_incomplete]: incomplete cache entry at {}",
            cache_root.display()
        )
        .into());
    }
    let registry_cache = cache_root
        .parent()
        .ok_or_else(|| "Registry cache path has no parent".to_string())?;
    fs::create_dir_all(registry_cache)
        .map_err(|error| format!("cannot create {}: {error}", registry_cache.display()))?;
    let temporary = registry_cache.join(format!(
        ".{archive_sha256}.tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&temporary)
        .map_err(|error| format!("cannot create {}: {error}", temporary.display()))?;
    let result: RegistryResult<()> = (|| {
        let temporary_archive = temporary.join("archive.tar.zst");
        fs::write(&temporary_archive, &download.bytes)
            .map_err(|error| format!("cannot populate Registry cache: {error}"))?;
        verify_digest(
            "cached package archive",
            archive_sha256,
            &hash_file(&temporary_archive)?,
        )?;
        terlan_archive::extract_tar_zstd(&temporary_archive, &temporary.join("source"))
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, &cache_root)
            .map_err(|error| format!("cannot publish Registry cache: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

#[cfg(test)]
#[path = "package_registry_resolver_test.rs"]
mod tests;
