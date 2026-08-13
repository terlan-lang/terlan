use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::terlan_quality::{render_failure, QualityResult};

const TOKIO_INVENTORY_PATH: &str = "tools/quality/tokio_runtime_inventory.tsv";
const CARGO_MANIFEST_PATH: &str = "crates/terlan/Cargo.toml";

const INVENTORY_HEADER: &[&str] = &["path", "classification", "owner", "notes"];

const SCAN_ROOTS: &[&str] = &[
    "Cargo.lock",
    "crates/terlan/Cargo.toml",
    "crates/terlan/src",
    "std",
];

const ALLOWED_CLASSIFICATIONS: &[&str] = &[
    "editor-tooling",
    "lockfile-transitive",
    "migration-debt",
    "quality-gate",
    "test-harness",
];

const ALLOWED_DIRECT_TOKIO_DEPENDENCIES: &[&str] = &[];
const ALLOWED_DIRECT_TOKIO_FEATURES: &[&str] = &["rt"];
const PLACEHOLDER_INVENTORY_VALUES: &[&str] = &["todo", "tbd", "unknown", "fixme"];

const REMOVED_DIRECT_RUNTIME_DEPENDENCIES: &[&str] = &["futures-util", "tokio-tungstenite"];
const REMOVED_DIRECT_TOKIO_FEATURES: &[&str] = &[
    "io-std",
    "io-util",
    "macros",
    "net",
    "rt-multi-thread",
    "sync",
    "time",
];

const ALLOWED_TEST_HARNESS_PATHS: &[&str] = &[
    "crates/terlan/src/benchmark/axum_baseline.rs",
    "crates/terlan/src/benchmark/http_framework_baseline.rs",
    "crates/terlan/src/benchmark/http_paired_benchmark.rs",
    "crates/terlan/src/benchmark/hyper_baseline.rs",
    "crates/terlan/src/benchmark/main.rs",
    "crates/terlan/src/commands/serve/serve_test.rs",
    "crates/terlan/src/lsp/lib_test.rs",
    "crates/terlan/src/lsp/lib_test/completion_inventory.rs",
    "crates/terlan/src/lsp/lib_test/completion_ranking.rs",
    "crates/terlan/src/lsp/lib_test/documents_and_shapes.rs",
    "crates/terlan/src/lsp/lib_test/parse_diagnostics.rs",
    "crates/terlan/src/lsp/lib_test/resolution_diagnostics.rs",
    "crates/terlan/src/lsp/lib_test/semantic_tokens_and_diagnostic_support.rs",
    "crates/terlan/src/lsp/lib_test/signatures_symbols_and_local_navigation.rs",
    "crates/terlan/src/lsp/lib_test/support.rs",
    "crates/terlan/src/lsp/lib_test/type_diagnostics.rs",
];

const ALLOWED_QUALITY_GATE_PATHS: &[&str] = &[
    "crates/terlan/src/quality/cli.rs",
    "crates/terlan/src/quality/cli/runtime_and_release_commands.rs",
    "crates/terlan/src/quality/mod.rs",
    "crates/terlan/src/quality/no_default_tokio_runtime.rs",
    "crates/terlan/src/quality/no_default_tokio_runtime_test.rs",
    "crates/terlan/src/quality/no_default_tokio_runtime_test/dependency_features.rs",
    "crates/terlan/src/quality/no_default_tokio_runtime_test/inventory_contract.rs",
    "crates/terlan/src/quality/rust_quality.rs",
    "crates/terlan/src/quality/vm_io_reactor_runtime.rs",
    "crates/terlan/src/quality/vm_io_reactor_runtime_test.rs",
    "crates/terlan/src/quality/vm_native_worker_runtime.rs",
];

const SERVE_TLS_MIGRATION_PATH: &str = "crates/terlan/src/commands/serve/tls/acme_runtime.rs";

/// Summary produced by the no-default-Tokio runtime gate.
///
/// Inputs:
/// - Inventory rows and scanned repository files containing Tokio references.
///
/// Output:
/// - Stable counts for CLI reporting.
///
/// Transformation:
/// - Keeps Tokio references visible and classified while preventing Tokio from
///   becoming an implicit default runtime contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoDefaultTokioRuntimeSummary {
    pub inventory_row_count: usize,
    pub scanned_reference_count: usize,
    pub direct_tokio_dependency_count: usize,
    pub direct_tokio_dependencies: Vec<String>,
}

/// One Tokio inventory row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokioInventoryRow {
    path: PathBuf,
    classification: String,
    owner: String,
    notes: String,
}

/// Runs the no-default-Tokio runtime gate.
///
/// Inputs:
/// - `root`: repository root containing Cargo metadata, source, std files, and
///   `tools/quality/tokio_runtime_inventory.tsv`.
///
/// Output:
/// - Success summary when every Tokio reference is explicitly classified.
/// - Stable diagnostics for unclassified, stale, or invalid inventory rows.
///
/// Transformation:
/// - Scans current repository text for Tokio references, compares the result to
///   the checked inventory, and rejects Tokio references inside VM-owned
///   runtime paths.
pub fn run_no_default_tokio_runtime(root: &Path) -> QualityResult<NoDefaultTokioRuntimeSummary> {
    let inventory = read_tokio_inventory(root)?;
    let references = collect_tokio_reference_files(root)?;
    let direct_dependencies = collect_direct_tokio_dependency_entries(root)?;
    let removed_tokio_features = collect_removed_direct_tokio_features(root)?;
    let unexpected_tokio_features = collect_unexpected_direct_tokio_features(root)?;
    let removed_dependencies = collect_removed_runtime_dependency_entries(root)?;
    let mut diagnostics = validate_allowed_classifications_have_no_placeholders();
    diagnostics.extend(validate_tokio_inventory(root, &inventory, &references));
    diagnostics.extend(validate_direct_tokio_dependencies(
        &inventory,
        &direct_dependencies,
    ));
    diagnostics.extend(validate_removed_direct_tokio_features(
        &removed_tokio_features,
    ));
    diagnostics.extend(validate_unexpected_direct_tokio_features(
        &unexpected_tokio_features,
    ));
    diagnostics.extend(validate_removed_runtime_dependencies(&removed_dependencies));
    if !diagnostics.is_empty() {
        return Err(render_failure("no-default-tokio-runtime", &diagnostics));
    }

    Ok(NoDefaultTokioRuntimeSummary {
        inventory_row_count: inventory.len(),
        scanned_reference_count: references.len(),
        direct_tokio_dependency_count: direct_dependencies.len(),
        direct_tokio_dependencies: direct_dependencies,
    })
}

/// Reads the Tokio runtime inventory TSV.
fn read_tokio_inventory(root: &Path) -> QualityResult<Vec<TokioInventoryRow>> {
    let text = fs::read_to_string(root.join(TOKIO_INVENTORY_PATH))
        .map_err(|err| format!("{TOKIO_INVENTORY_PATH}: failed to read inventory: {err}"))?;
    parse_tokio_inventory(&text)
}

/// Parses Tokio inventory TSV text.
fn parse_tokio_inventory(text: &str) -> QualityResult<Vec<TokioInventoryRow>> {
    let mut rows = uncommented_tsv_rows(text);
    let Some((line, header)) = rows.next() else {
        return Err(format!("{TOKIO_INVENTORY_PATH}: missing header"));
    };
    if header != INVENTORY_HEADER {
        return Err(format!(
            "{TOKIO_INVENTORY_PATH}:{line}: expected header `{}`, found `{}`",
            INVENTORY_HEADER.join("\t"),
            header.join("\t")
        ));
    }

    let mut inventory = Vec::new();
    for (line, fields) in rows {
        if fields.len() != INVENTORY_HEADER.len() {
            return Err(format!(
                "{TOKIO_INVENTORY_PATH}:{line}: expected {} columns, found {}",
                INVENTORY_HEADER.len(),
                fields.len()
            ));
        }
        inventory.push(TokioInventoryRow {
            path: PathBuf::from(fields[0]),
            classification: fields[1].to_string(),
            owner: fields[2].to_string(),
            notes: fields[3].to_string(),
        });
    }
    Ok(inventory)
}

/// Returns non-comment TSV rows with one-based line numbers.
fn uncommented_tsv_rows(text: &str) -> impl Iterator<Item = (usize, Vec<&str>)> {
    text.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some((index + 1, line.split('\t').collect()))
        }
    })
}

/// Collects repository files that mention Tokio.
fn collect_tokio_reference_files(root: &Path) -> QualityResult<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for relative in SCAN_ROOTS {
        let path = root.join(relative);
        if path.is_file() {
            maybe_insert_tokio_file(root, Path::new(relative), &mut files)?;
        } else if path.is_dir() {
            collect_tokio_reference_files_in_dir(root, Path::new(relative), &mut files)?;
        }
    }
    Ok(files)
}

/// Recursively collects files that mention Tokio under one directory.
fn collect_tokio_reference_files_in_dir(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    let full_path = root.join(relative);
    for entry in fs::read_dir(&full_path)
        .map_err(|err| format!("{}: failed to read directory: {err}", relative.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "{}: failed to read directory entry: {err}",
                relative.display()
            )
        })?;
        let child = relative.join(entry.file_name());
        let child_full_path = root.join(&child);
        if child_full_path.is_dir() {
            if should_skip_dir(&child) {
                continue;
            }
            collect_tokio_reference_files_in_dir(root, &child, files)?;
        } else if child_full_path.is_file() {
            maybe_insert_tokio_file(root, &child, files)?;
        }
    }
    Ok(())
}

/// Inserts one file into the reference set when its text mentions Tokio.
fn maybe_insert_tokio_file(
    root: &Path,
    relative: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> QualityResult<()> {
    let text = match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => return Ok(()),
        Err(err) => {
            return Err(format!(
                "{}: failed to read scanned file: {err}",
                relative.display()
            ));
        }
    };
    if text.to_lowercase().contains("tokio") {
        files.insert(relative.to_path_buf());
    }
    Ok(())
}

/// Validates inventory rows against scanned Tokio references.
fn validate_tokio_inventory(
    root: &Path,
    inventory: &[TokioInventoryRow],
    references: &BTreeSet<PathBuf>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut by_path = BTreeMap::new();

    for row in inventory {
        if by_path.insert(row.path.clone(), row).is_some() {
            diagnostics.push(format!(
                "{}: duplicate Tokio inventory row",
                row.path.display()
            ));
        }
        if !ALLOWED_CLASSIFICATIONS.contains(&row.classification.as_str()) {
            diagnostics.push(format!(
                "{}: unsupported Tokio classification `{}`",
                row.path.display(),
                row.classification
            ));
        }
        if let Err(diagnostic) = validate_classification_scope(row) {
            diagnostics.push(diagnostic);
        }
        if row.owner.trim().is_empty() || row.notes.trim().is_empty() {
            diagnostics.push(format!(
                "{}: Tokio inventory rows require owner and notes",
                row.path.display()
            ));
        }
        if is_placeholder_inventory_value(&row.owner) || is_placeholder_inventory_value(&row.notes)
        {
            diagnostics.push(format!(
                "{}: Tokio inventory owner and notes must not use placeholder values",
                row.path.display()
            ));
        }
        if !root.join(&row.path).exists() {
            diagnostics.push(format!(
                "{}: stale Tokio inventory path",
                row.path.display()
            ));
        }
        if row.path.starts_with("crates/terlan/src/vm")
            || row.path.starts_with("crates/terlan/src/runtime/vm")
        {
            diagnostics.push(format!(
                "{}: VM-owned runtime paths must not depend on Tokio",
                row.path.display()
            ));
        }
        if path_is_serve_tls_migration_boundary(&row.path) {
            diagnostics.extend(validate_serve_tls_tokio_boundary(root, &row.path));
        }
    }

    for reference in references {
        if !by_path.contains_key(reference) {
            diagnostics.push(format!(
                "{}: unclassified Tokio reference",
                reference.display()
            ));
        }
    }

    for row in inventory {
        if root.join(&row.path).exists() && !references.contains(&row.path) {
            diagnostics.push(format!(
                "{}: stale Tokio inventory row; file no longer mentions Tokio",
                row.path.display()
            ));
        }
    }

    diagnostics
}

fn is_placeholder_inventory_value(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    PLACEHOLDER_INVENTORY_VALUES
        .iter()
        .any(|placeholder| normalized == *placeholder || normalized.contains(placeholder))
}

fn validate_allowed_classifications_have_no_placeholders() -> Vec<String> {
    ALLOWED_CLASSIFICATIONS
        .iter()
        .flat_map(|classification| {
            validate_text_has_no_placeholder_value("allowed Tokio classification", classification)
        })
        .collect()
}

fn validate_text_has_no_placeholder_value(label: &str, value: &str) -> Vec<String> {
    if is_placeholder_inventory_value(value) {
        vec![format!(
            "{label} `{value}` must not use placeholder inventory values"
        )]
    } else {
        Vec::new()
    }
}

/// Collects direct Tokio-family dependencies from the Terlan Cargo manifest.
fn collect_direct_tokio_dependency_entries(root: &Path) -> QualityResult<Vec<String>> {
    let manifest = fs::read_to_string(root.join(CARGO_MANIFEST_PATH))
        .map_err(|err| format!("{CARGO_MANIFEST_PATH}: failed to read manifest: {err}"))?;
    parse_direct_tokio_dependency_entries_checked(&manifest)
}

/// Parses direct dependencies that either name Tokio or enable Tokio features.
#[cfg(test)]
fn parse_direct_tokio_dependency_entries(manifest: &str) -> Vec<String> {
    parse_direct_tokio_dependency_entries_checked(manifest).unwrap_or_default()
}

/// Parses direct dependencies with manifest syntax diagnostics.
fn parse_direct_tokio_dependency_entries_checked(manifest: &str) -> QualityResult<Vec<String>> {
    let mut entries = Vec::new();
    for dependency in parse_default_dependency_entries(manifest)? {
        if dependency.package_name.contains("tokio") {
            entries.push(dependency.package_name);
        } else if dependency.dependency_name.contains("tokio") {
            entries.push(dependency.dependency_name);
        } else if dependency.features.iter().any(|feature| feature == "tokio") {
            entries.push(format!("{}[feature:tokio]", dependency.package_name));
        }
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

/// Collects removed direct Tokio features from the Terlan Cargo manifest.
fn collect_removed_direct_tokio_features(root: &Path) -> QualityResult<Vec<String>> {
    let manifest = fs::read_to_string(root.join(CARGO_MANIFEST_PATH))
        .map_err(|err| format!("{CARGO_MANIFEST_PATH}: failed to read manifest: {err}"))?;
    parse_removed_direct_tokio_features_checked(&manifest)
}

/// Parses direct Tokio features that have already been removed.
#[cfg(test)]
fn parse_removed_direct_tokio_features(manifest: &str) -> Vec<String> {
    parse_removed_direct_tokio_features_checked(manifest).unwrap_or_default()
}

/// Parses removed direct Tokio features with manifest syntax diagnostics.
fn parse_removed_direct_tokio_features_checked(manifest: &str) -> QualityResult<Vec<String>> {
    let mut entries = Vec::new();
    for dependency in parse_default_dependency_entries(manifest)? {
        if dependency.package_name == "tokio" || dependency.dependency_name == "tokio" {
            for feature in dependency.features {
                if REMOVED_DIRECT_TOKIO_FEATURES.contains(&feature.as_str()) {
                    entries.push(feature);
                }
            }
        }
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

/// Collects unapproved direct Tokio features from the Terlan Cargo manifest.
fn collect_unexpected_direct_tokio_features(root: &Path) -> QualityResult<Vec<String>> {
    let manifest = fs::read_to_string(root.join(CARGO_MANIFEST_PATH))
        .map_err(|err| format!("{CARGO_MANIFEST_PATH}: failed to read manifest: {err}"))?;
    parse_unexpected_direct_tokio_features_checked(&manifest)
}

/// Parses direct Tokio features outside the current allowlist.
#[cfg(test)]
fn parse_unexpected_direct_tokio_features(manifest: &str) -> Vec<String> {
    parse_unexpected_direct_tokio_features_checked(manifest).unwrap_or_default()
}

/// Parses unexpected direct Tokio features with manifest syntax diagnostics.
fn parse_unexpected_direct_tokio_features_checked(manifest: &str) -> QualityResult<Vec<String>> {
    let mut entries = Vec::new();
    for dependency in parse_default_dependency_entries(manifest)? {
        if dependency.package_name == "tokio" || dependency.dependency_name == "tokio" {
            for feature in dependency.features {
                if !ALLOWED_DIRECT_TOKIO_FEATURES.contains(&feature.as_str())
                    && !REMOVED_DIRECT_TOKIO_FEATURES.contains(&feature.as_str())
                {
                    entries.push(feature);
                }
            }
        }
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

/// Collects removed runtime dependencies from the Terlan Cargo manifest.
fn collect_removed_runtime_dependency_entries(root: &Path) -> QualityResult<Vec<String>> {
    let manifest = fs::read_to_string(root.join(CARGO_MANIFEST_PATH))
        .map_err(|err| format!("{CARGO_MANIFEST_PATH}: failed to read manifest: {err}"))?;
    parse_removed_runtime_dependency_entries_checked(&manifest)
}

/// Parses default dependencies that have already been removed from runtime lanes.
#[cfg(test)]
fn parse_removed_runtime_dependency_entries(manifest: &str) -> Vec<String> {
    parse_removed_runtime_dependency_entries_checked(manifest).unwrap_or_default()
}

/// Parses removed runtime dependencies with manifest syntax diagnostics.
fn parse_removed_runtime_dependency_entries_checked(manifest: &str) -> QualityResult<Vec<String>> {
    let mut entries = parse_default_dependency_entries(manifest)?
        .into_iter()
        .filter_map(|dependency| {
            if REMOVED_DIRECT_RUNTIME_DEPENDENCIES.contains(&dependency.package_name.as_str()) {
                Some(dependency.package_name)
            } else if REMOVED_DIRECT_RUNTIME_DEPENDENCIES
                .contains(&dependency.dependency_name.as_str())
            {
                Some(dependency.dependency_name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.dedup();
    Ok(entries)
}

/// Describes a default Cargo dependency entry after alias resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DefaultDependencyEntry {
    dependency_name: String,
    package_name: String,
    features: Vec<String>,
}

/// Cargo manifest subset used by the default runtime dependency gate.
#[derive(Debug, Deserialize)]
struct CargoManifest {
    #[serde(default)]
    dependencies: BTreeMap<String, CargoDependency>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
}

/// Cargo dependency declaration shape accepted in `[dependencies]`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CargoDependency {
    Table(CargoDependencyTable),
    Version(serde::de::IgnoredAny),
}

/// Cargo dependency table fields used by the gate.
#[derive(Debug, Default, Deserialize)]
struct CargoDependencyTable {
    package: Option<String>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    features: Vec<String>,
}

/// Parses package default dependency names and specifications through TOML.
fn parse_default_dependency_entries(manifest: &str) -> QualityResult<Vec<DefaultDependencyEntry>> {
    let manifest = basic_toml::from_str::<CargoManifest>(manifest)
        .map_err(|err| format!("{CARGO_MANIFEST_PATH}: invalid Cargo manifest: {err}"))?;
    let default_enabled_dependencies = default_enabled_optional_dependencies(&manifest.features);
    Ok(manifest
        .dependencies
        .into_iter()
        .filter_map(|(dependency_name, dependency)| {
            let (package_name, features) = match dependency {
                CargoDependency::Version(_) => (dependency_name.clone(), Vec::new()),
                CargoDependency::Table(table) => {
                    if table.optional && !default_enabled_dependencies.contains(&dependency_name) {
                        return None;
                    }
                    (
                        table.package.unwrap_or_else(|| dependency_name.clone()),
                        table.features,
                    )
                }
            };
            Some(DefaultDependencyEntry {
                dependency_name,
                package_name,
                features,
            })
        })
        .collect())
}

/// Returns optional dependency names enabled by the Cargo default feature.
fn default_enabled_optional_dependencies(
    features: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();
    let mut visited_features = BTreeSet::new();
    let mut pending = features.get("default").cloned().unwrap_or_default();
    while let Some(entry) = pending.pop() {
        let feature_or_dependency = entry
            .strip_prefix("dep:")
            .unwrap_or(entry.as_str())
            .split('/')
            .next()
            .unwrap_or_default()
            .trim_end_matches('?');
        if feature_or_dependency.is_empty() {
            continue;
        }
        if entry.starts_with("dep:") {
            dependencies.insert(feature_or_dependency.to_string());
        } else if let Some(nested) = features.get(feature_or_dependency) {
            if visited_features.insert(feature_or_dependency.to_string()) {
                pending.extend(nested.iter().cloned());
            }
        } else {
            dependencies.insert(feature_or_dependency.to_string());
        }
    }
    dependencies
}

/// Validates direct Tokio dependencies are inventoried and allowlisted.
fn validate_direct_tokio_dependencies(
    inventory: &[TokioInventoryRow],
    direct_dependencies: &[String],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let cargo_row = inventory
        .iter()
        .find(|row| row.path == Path::new(CARGO_MANIFEST_PATH));
    if !direct_dependencies.is_empty() {
        match cargo_row {
            Some(row) if row.classification == "migration-debt" => {
                diagnostics.extend(validate_direct_tokio_dependency_plan(
                    row,
                    direct_dependencies,
                ));
            }
            Some(row) => diagnostics.push(format!(
                "{CARGO_MANIFEST_PATH}: direct Tokio dependencies require `migration-debt`, found `{}`",
                row.classification
            )),
            None => diagnostics.push(format!(
                "{CARGO_MANIFEST_PATH}: direct Tokio dependencies require an inventory row"
            )),
        }
    }

    for dependency in direct_dependencies {
        if !ALLOWED_DIRECT_TOKIO_DEPENDENCIES.contains(&dependency.as_str()) {
            diagnostics.push(format!(
                "{CARGO_MANIFEST_PATH}: unexpected direct Tokio dependency `{dependency}`"
            ));
        }
    }

    diagnostics
}

/// Validates removed runtime dependencies do not re-enter default dependencies.
fn validate_removed_runtime_dependencies(removed_dependencies: &[String]) -> Vec<String> {
    removed_dependencies
        .iter()
        .map(|dependency| {
            format!(
                "{CARGO_MANIFEST_PATH}: removed runtime dependency `{dependency}` must stay absent"
            )
        })
        .collect()
}

/// Validates removed direct Tokio features do not re-enter default deps.
fn validate_removed_direct_tokio_features(removed_features: &[String]) -> Vec<String> {
    removed_features
        .iter()
        .map(|feature| {
            format!(
                "{CARGO_MANIFEST_PATH}: removed Tokio feature `{feature}` must stay absent from the direct `tokio` dependency"
            )
        })
        .collect()
}

/// Validates direct Tokio dependency features stay in the exact allowlist.
fn validate_unexpected_direct_tokio_features(features: &[String]) -> Vec<String> {
    features
        .iter()
        .map(|feature| {
            format!(
                "{CARGO_MANIFEST_PATH}: unexpected Tokio feature `{feature}` must not enter the direct default `tokio` dependency; allowed features: {}",
                ALLOWED_DIRECT_TOKIO_FEATURES.join(", ")
            )
        })
        .collect()
}

/// Validates that the Cargo manifest inventory row is an actionable removal plan.
fn validate_direct_tokio_dependency_plan(
    row: &TokioInventoryRow,
    direct_dependencies: &[String],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let notes = row.notes.to_ascii_lowercase();
    if !notes.contains("remove") && !notes.contains("removal") && !notes.contains("replace") {
        diagnostics.push(format!(
            "{CARGO_MANIFEST_PATH}: direct Tokio dependency inventory notes must describe removal or replacement"
        ));
    }
    for dependency in direct_dependencies {
        if !row.notes.contains(dependency) {
            diagnostics.push(format!(
                "{CARGO_MANIFEST_PATH}: direct Tokio dependency inventory notes must name `{dependency}`"
            ));
        }
    }
    let direct_dependency_set = direct_dependencies.iter().collect::<BTreeSet<_>>();
    for planned_dependency in planned_direct_tokio_dependencies(&row.notes) {
        if !direct_dependency_set.contains(&planned_dependency) {
            diagnostics.push(format!(
                "{CARGO_MANIFEST_PATH}: stale direct Tokio dependency `{planned_dependency}` in removal plan"
            ));
        }
    }
    diagnostics
}

/// Extracts dependency names from `Removal plan: dependency -> action; ...`
/// notes.
fn planned_direct_tokio_dependencies(notes: &str) -> Vec<String> {
    let Some((_, plan)) = notes.split_once("Removal plan:") else {
        return Vec::new();
    };
    plan.split(';')
        .filter_map(|entry| {
            let (dependency, _) = entry.split_once("->")?;
            let dependency = dependency.trim();
            (!dependency.is_empty()).then(|| dependency.to_string())
        })
        .collect()
}

/// Validates that a Tokio inventory classification is scoped to its lane.
fn validate_classification_scope(row: &TokioInventoryRow) -> Result<(), String> {
    let valid = match row.classification.as_str() {
        "editor-tooling" => row.path == Path::new("crates/terlan/src/lsp/server.rs"),
        "lockfile-transitive" => row.path == Path::new("Cargo.lock"),
        "quality-gate" => ALLOWED_QUALITY_GATE_PATHS
            .iter()
            .any(|allowed| row.path == Path::new(allowed)),
        "test-harness" => path_is_test_harness(&row.path),
        "migration-debt" => path_is_migration_debt_boundary(&row.path),
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{}: Tokio classification `{}` is not allowed for this path",
            row.path.display(),
            row.classification
        ))
    }
}

/// Returns whether a path is an explicit test-only Tokio lane.
fn path_is_test_harness(path: &Path) -> bool {
    ALLOWED_TEST_HARNESS_PATHS
        .iter()
        .any(|allowed| path == Path::new(allowed))
}

/// Returns whether a path is an explicitly retained migration boundary.
fn path_is_migration_debt_boundary(path: &Path) -> bool {
    path == Path::new("crates/terlan/Cargo.toml") || path_is_serve_tls_migration_boundary(path)
}

fn path_is_serve_tls_migration_boundary(path: &Path) -> bool {
    path == Path::new(SERVE_TLS_MIGRATION_PATH)
}

/// Validates the temporary serve TLS Tokio bridge is limited to live ACME.
///
/// Inputs:
/// - `root`: repository root containing the serve TLS source file.
///
/// Output:
/// - Empty diagnostics when the only `tokio::` use is the temporary runtime
///   that drives `issue_acme_certificate_cache`.
/// - Stable diagnostics when serve TLS grows broader Tokio runtime behavior.
///
/// Transformation:
/// - Keeps the remaining serve-side migration debt narrow while ACME issuance
///   waits for a VM worker or NativeBoundary adapter.
fn validate_serve_tls_tokio_boundary(root: &Path, path: &Path) -> Vec<String> {
    let relative = path.display();
    let text = match fs::read_to_string(root.join(path)) {
        Ok(text) => text,
        Err(err) => {
            return vec![format!(
                "{relative}: failed to read serve TLS migration boundary: {err}"
            )];
        }
    };
    let mut diagnostics = Vec::new();
    let tokio_use_count = text.matches("tokio::").count();
    if tokio_use_count != 1 {
        diagnostics.push(format!(
            "{relative}: serve TLS migration boundary may contain exactly one `tokio::` use for live ACME issuance, found {tokio_use_count}"
        ));
    }
    if !text.contains("tokio::runtime::Builder::new_current_thread()") {
        diagnostics.push(format!(
            "{relative}: serve TLS Tokio usage must be the temporary live ACME runtime builder"
        ));
    }
    if !text.contains("runtime.block_on(issue_acme_certificate_cache(plan))") {
        diagnostics.push(format!(
            "{relative}: serve TLS Tokio runtime must only drive `issue_acme_certificate_cache`"
        ));
    }
    diagnostics
}

/// Returns whether a directory should be excluded from Tokio scanning.
fn should_skip_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), "target" | "node_modules" | ".git")
    })
}

#[cfg(test)]
#[path = "no_default_tokio_runtime_test.rs"]
#[cfg(test)]
mod no_default_tokio_runtime_test;
