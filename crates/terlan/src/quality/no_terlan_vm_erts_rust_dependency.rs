use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::terlan_quality::{render_failure, QualityResult};

const QUARANTINE_DOC: &str = "docs/runtime/TERLAN_VM_ERTS_RUST_QUARANTINE.md";
const RETIRED_ERTS_RUST_DIR: &str = "terlan-vm/erts/rust";
const MIGRATION_INVENTORY: &str = "terlan-vm/erts/rust/MIGRATION_INVENTORY.tsv";

const REQUIRED_DOC_TERMS: &[&str] = &[
    "quarantined migration reference material",
    "not wired into `make check`",
    "not release-blocking",
    "not part of the default compiler release graph",
    "Reference/history only",
    "temporary Cargo target directory outside the source tree",
    "deleted after VM-owned code is ported",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

const RETIRED_DEFAULT_GATES: &[&str] = &[
    "terlan-vm-erts-rust-check",
    "terlan-vm-rust-core-slice-check",
    "terlan-vm-rust-deferred-inventory-check",
];

const DEFAULT_TARGETS: &[&str] = &[
    "check",
    "test",
    "test-release",
    "release-artifact-current",
    "release-artifact-linux",
    "release-artifact-smoke",
    "release-artifact-installer-smoke",
    "publish-preflight",
    "publish",
];

const QUARANTINED_CARGO_TARGET_DIRS: &[&str] = &[
    "/tmp/terlan-vm-erts-rust-target",
    "/tmp/terlan-vm-rust-core-slice-target",
];

const ALLOWED_INVENTORY_CLASSIFICATIONS: &[&str] = &[
    "vm-owned",
    "test-support",
    "runtime-helper",
    "transitional-abi-adapter",
    "reference-only",
    "out-of-contract",
];

const ALLOWED_INVENTORY_POLICIES: &[&str] = &["migrate-first", "deferred"];

const ALLOWED_MIGRATE_FIRST_CRATES: &[&str] = &["epmd", "terlan_erts_test_support", "terlan_vm"];
const INVENTORY_HEADER: &str = "crate\tclassification\tmigration_policy\tgolden_evidence";

/// Summary produced by the ERTS Rust dependency quarantine gate.
///
/// Inputs:
/// - Makefile target bodies.
/// - The ERTS Rust quarantine documentation.
///
/// Output:
/// - Stable counts for CLI reporting.
///
/// Transformation:
/// - Keeps retained migration targets available for manual reference while
///   proving default checks and release gates do not depend on
///   `terlan-vm/erts/rust`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoTerlanVmErtsRustDependencySummary {
    pub retired_gate_count: usize,
    pub checked_default_target_count: usize,
    pub retained_inventory_count: usize,
}

/// Runs the ERTS Rust dependency quarantine gate.
///
/// Inputs:
/// - `root`: repository root containing the Makefile and runtime docs.
///
/// Output:
/// - Success summary when the old ERTS Rust tree is quarantined out of the
///   default release graph.
/// - Stable diagnostics when default targets depend on the retired tree or
///   retained targets write build output into that tree.
///
/// Transformation:
/// - Parses Make target bodies directly, validates the quarantine contract, and
///   rejects source-tree Cargo target output for retained migration targets.
pub fn run_no_terlan_vm_erts_rust_dependency(
    root: &Path,
) -> QualityResult<NoTerlanVmErtsRustDependencySummary> {
    let makefile = read_repo_text(root, "Makefile")?;
    let quarantine_doc = read_repo_text(root, QUARANTINE_DOC)?;
    let targets = parse_make_targets(&makefile);

    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_quarantine_doc_text(&quarantine_doc));
    diagnostics.extend(validate_retired_target_definitions_absent(&targets));
    diagnostics.extend(validate_default_targets(&targets));
    diagnostics.extend(validate_quarantined_targets(&targets));
    diagnostics.extend(validate_retained_tree_inventory(root));

    if !diagnostics.is_empty() {
        return Err(render_failure(
            "no-terlan-vm-erts-rust-dependency",
            &diagnostics,
        ));
    }

    Ok(NoTerlanVmErtsRustDependencySummary {
        retired_gate_count: RETIRED_DEFAULT_GATES.len(),
        checked_default_target_count: DEFAULT_TARGETS.len(),
        retained_inventory_count: retained_inventory_count(root),
    })
}

/// Validates required quarantine wording.
fn validate_quarantine_doc_text(text: &str) -> Vec<String> {
    let normalized = normalize_text(text);
    let mut diagnostics = REQUIRED_DOC_TERMS
        .iter()
        .filter(|term| !normalized.contains(&normalize_text(term)))
        .map(|term| format!("missing ERTS Rust quarantine term `{term}`"))
        .collect::<Vec<_>>();
    diagnostics.extend(
        PLACEHOLDER_TERMS
            .iter()
            .filter(|term| normalized.contains(&normalize_text(term)))
            .map(|term| format!("placeholder ERTS Rust quarantine text `{term}` is not allowed")),
    );
    diagnostics
}

/// Validates default targets no longer depend on retired ERTS Rust checks.
fn validate_default_targets(targets: &BTreeMap<String, String>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for target in DEFAULT_TARGETS {
        let Some(body) = targets.get(*target) else {
            diagnostics.push(format!("Makefile target `{target}` is missing"));
            continue;
        };
        for gate in RETIRED_DEFAULT_GATES {
            if body.contains(gate) {
                diagnostics.push(format!(
                    "`make {target}` must not invoke retired ERTS Rust gate `{gate}`"
                ));
            }
        }
        if body.contains("terlan-vm/erts/rust") {
            diagnostics.push(format!(
                "`make {target}` must not depend on `terlan-vm/erts/rust`"
            ));
        }
    }
    diagnostics
}

/// Validates retired ERTS Rust Make targets are no longer public commands.
fn validate_retired_target_definitions_absent(targets: &BTreeMap<String, String>) -> Vec<String> {
    RETIRED_DEFAULT_GATES
        .iter()
        .filter(|gate| targets.contains_key(**gate))
        .map(|gate| format!("retired ERTS Rust target `{gate}` must not be defined"))
        .collect()
}

/// Validates retained ERTS Rust targets are quarantined and write outside tree.
fn validate_quarantined_targets(targets: &BTreeMap<String, String>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for gate in RETIRED_DEFAULT_GATES {
        let Some(body) = targets.get(*gate) else {
            continue;
        };
        if target_body_writes_erts_rust_target(body) {
            diagnostics.push(format!(
                "quarantined target `{gate}` must not write to `terlan-vm/erts/rust/target`"
            ));
        }
        if body.contains("cargo") || body.contains("$(CARGO)") {
            let uses_tmp_target = QUARANTINED_CARGO_TARGET_DIRS
                .iter()
                .any(|target_dir| body.contains(target_dir));
            if !uses_tmp_target {
                diagnostics.push(format!(
                    "quarantined Cargo target `{gate}` must use a /tmp Cargo target directory"
                ));
            }
        }
    }
    diagnostics
}

/// Returns true when a target body writes Cargo output into the retired tree.
fn target_body_writes_erts_rust_target(body: &str) -> bool {
    body.contains("CARGO_TARGET_DIR=terlan-vm/erts/rust/target")
        || body.contains("CARGO_TARGET_DIR=\"terlan-vm/erts/rust/target\"")
        || body.contains("mkdir -p terlan-vm/erts/rust/target")
}

/// Validates the retained ERTS Rust tree has an explicit migration inventory.
fn validate_retained_tree_inventory(root: &Path) -> Vec<String> {
    let retained_root = root.join(RETIRED_ERTS_RUST_DIR);
    if !retained_root.exists() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    let inventory_path = root.join(MIGRATION_INVENTORY);
    let Ok(inventory) = fs::read_to_string(&inventory_path) else {
        return vec![format!(
            "retained `{RETIRED_ERTS_RUST_DIR}` tree requires `{MIGRATION_INVENTORY}`"
        )];
    };

    let rows = parse_migration_inventory(&inventory, &mut diagnostics);
    diagnostics.extend(validate_inventory_rows(&rows));
    diagnostics.extend(validate_migrate_first_golden_evidence(root, &rows));
    diagnostics.extend(validate_inventory_matches_retained_dirs(
        &retained_root,
        &rows,
    ));
    diagnostics
}

/// Parses the ERTS Rust migration inventory.
fn parse_migration_inventory(
    inventory: &str,
    diagnostics: &mut Vec<String>,
) -> Vec<MigrationInventoryRow> {
    let mut rows = Vec::new();
    let mut lines = inventory.lines();
    let header = lines.next().unwrap_or_default();
    if header != INVENTORY_HEADER {
        diagnostics.push(format!(
            "`{MIGRATION_INVENTORY}` must start with `crate\\tclassification\\tmigration_policy\\tgolden_evidence`"
        ));
    }

    for (offset, line) in lines.enumerate() {
        let line_number = offset + 2;
        if line.trim().is_empty() {
            continue;
        }
        let columns: Vec<_> = line.split('\t').collect();
        if columns.len() != 4 {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` line {line_number} must have 4 tab-separated columns"
            ));
            continue;
        }
        rows.push(MigrationInventoryRow {
            crate_name: columns[0].to_string(),
            classification: columns[1].to_string(),
            migration_policy: columns[2].to_string(),
            golden_evidence: columns[3].to_string(),
        });
    }

    rows
}

/// Validates inventory row shape and retained migration evidence policy.
fn validate_inventory_rows(rows: &[MigrationInventoryRow]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeMap::new();

    for row in rows {
        if row.crate_name.trim().is_empty() {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` contains an empty crate name"
            ));
            continue;
        }
        if seen.insert(row.crate_name.clone(), ()).is_some() {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` contains duplicate crate `{}`",
                row.crate_name
            ));
        }
        if !ALLOWED_INVENTORY_CLASSIFICATIONS.contains(&row.classification.as_str()) {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` crate `{}` has invalid classification `{}`",
                row.crate_name, row.classification
            ));
        }
        if !ALLOWED_INVENTORY_POLICIES.contains(&row.migration_policy.as_str()) {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` crate `{}` has invalid migration policy `{}`",
                row.crate_name, row.migration_policy
            ));
        }
        if row.migration_policy == "migrate-first"
            && !ALLOWED_MIGRATE_FIRST_CRATES.contains(&row.crate_name.as_str())
        {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` crate `{}` cannot be `migrate-first`; retained first-migration evidence is limited to {}",
                row.crate_name,
                ALLOWED_MIGRATE_FIRST_CRATES.join(", ")
            ));
        }
    }

    diagnostics
}

/// Validates migrate-first rows point at golden-owned replacement evidence.
fn validate_migrate_first_golden_evidence(
    root: &Path,
    rows: &[MigrationInventoryRow],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for row in rows
        .iter()
        .filter(|row| row.migration_policy == "migrate-first")
    {
        if row.golden_evidence.trim().is_empty() || row.golden_evidence == "-" {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` migrate-first crate `{}` must name a golden-owned evidence path",
                row.crate_name
            ));
            continue;
        }
        if row.golden_evidence.starts_with(RETIRED_ERTS_RUST_DIR) {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` migrate-first crate `{}` evidence must not point back at `{RETIRED_ERTS_RUST_DIR}`",
                row.crate_name
            ));
            continue;
        }
        if !root.join(&row.golden_evidence).exists() {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` migrate-first crate `{}` evidence path `{}` does not exist",
                row.crate_name, row.golden_evidence
            ));
        }
    }
    diagnostics
}

/// Validates every retained crate directory is inventoried and nothing extra is.
fn validate_inventory_matches_retained_dirs(
    retained_root: &Path,
    rows: &[MigrationInventoryRow],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let Ok(entries) = fs::read_dir(retained_root) else {
        return vec![format!("failed to read `{RETIRED_ERTS_RUST_DIR}`")];
    };
    let mut retained_dirs = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                retained_dirs.insert(name.to_string(), ());
            }
        }
    }

    let inventoried = rows
        .iter()
        .map(|row| (row.crate_name.clone(), ()))
        .collect::<BTreeMap<_, _>>();

    for retained in retained_dirs.keys() {
        if !inventoried.contains_key(retained) {
            diagnostics.push(format!(
                "`{RETIRED_ERTS_RUST_DIR}/{retained}` is retained but missing from `{MIGRATION_INVENTORY}`"
            ));
        }
    }
    for inventoried_name in inventoried.keys() {
        if !retained_dirs.contains_key(inventoried_name) {
            diagnostics.push(format!(
                "`{MIGRATION_INVENTORY}` lists `{inventoried_name}` but `{RETIRED_ERTS_RUST_DIR}/{inventoried_name}` is absent"
            ));
        }
    }

    diagnostics
}

/// Counts retained inventory rows when the old tree is present.
fn retained_inventory_count(root: &Path) -> usize {
    if !root.join(RETIRED_ERTS_RUST_DIR).exists() {
        return 0;
    }
    let Ok(inventory) = fs::read_to_string(root.join(MIGRATION_INVENTORY)) else {
        return 0;
    };
    inventory
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// One row in the retained ERTS Rust migration inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationInventoryRow {
    crate_name: String,
    classification: String,
    migration_policy: String,
    golden_evidence: String,
}

/// Parses simple Make target bodies by target name.
fn parse_make_targets(makefile: &str) -> BTreeMap<String, String> {
    let mut targets = BTreeMap::new();
    let mut current_target: Option<String> = None;
    let mut current_body = String::new();

    for line in makefile.lines() {
        if let Some(target) = parse_target_name(line) {
            if let Some(previous) = current_target.replace(target) {
                targets.insert(previous, current_body);
                current_body = String::new();
            }
        }
        if current_target.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    if let Some(target) = current_target {
        targets.insert(target, current_body);
    }

    targets
}

/// Parses one simple Make target name.
fn parse_target_name(line: &str) -> Option<String> {
    if line.starts_with('\t')
        || line.starts_with('.')
        || line.starts_with('#')
        || line.trim().is_empty()
    {
        return None;
    }
    let (name, _) = line.split_once(':')?;
    if name.contains(' ') || name.contains('=') || name.contains('$') {
        return None;
    }
    Some(name.to_string())
}

/// Reads one repository text file.
fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read file: {err}"))
}

/// Normalizes text for stable term checks.
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "no_terlan_vm_erts_rust_dependency_test.rs"]
mod no_terlan_vm_erts_rust_dependency_test;
