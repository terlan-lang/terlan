use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use serde::Deserialize;

use crate::terlan_quality::QualityResult;

/// Repository-relative machine-readable inventory path.
const INVENTORY_PATH: &str = "docs/runtime/TVM_MULTICORE_INVARIANT_INVENTORY.json";
/// Repository-relative Terlan concurrency contract path.
const CONTRACT_PATH: &str = "docs/runtime/TVM_MULTICORE_CONCURRENCY_CONTRACT.md";
/// Frozen inventory schema identifier.
const SCHEMA: &str = "terlan.tvm-multicore-invariant-inventory.v1";
/// Pinned official OTP repository.
const OTP_REPOSITORY: &str = "https://github.com/erlang/otp";
/// Pinned OTP release tag used for MC-1 mining.
const OTP_TAG: &str = "OTP-29.0.1";
/// Pinned OTP commit used for MC-1 mining.
const OTP_REVISION: &str = "f26c7e590c5d1b3afa0dee38093442df117822e3";

/// Every concurrency domain required by MC-1.
const REQUIRED_DOMAINS: &[&str] = &[
    "run-queues",
    "lifecycle-ownership",
    "memory-publication",
    "work-stealing",
    "reductions",
    "migration",
    "mailboxes",
    "signals",
    "links",
    "monitors",
    "timers",
    "ports-io",
    "cancellation",
    "failure",
    "reclamation",
    "shutdown",
];

/// Allowed MC-1 disposition classes.
const CLASSIFICATIONS: &[&str] = &[
    "port-semantic-invariant",
    "port-adversarial-test",
    "terlan-different-api",
    "remove-erts-implementation-detail",
];

/// Frozen Terlan concurrency contract clause identities.
const CONTRACT_IDS: &[&str] = &[
    "MC-C01", "MC-C02", "MC-C03", "MC-C04", "MC-C05", "MC-C06", "MC-C07", "MC-C08", "MC-C09",
    "MC-C10", "MC-C11", "MC-C12",
];

/// Complete parsed MC-1 inventory.
#[derive(Debug, Clone, Deserialize)]
struct Inventory {
    /// Versioned artifact schema.
    schema: String,
    /// Pinned upstream provenance.
    upstream: Upstream,
    /// Repository-relative contract document.
    contract_document: String,
    /// Domains the artifact claims to cover.
    required_domains: Vec<String>,
    /// Classified upstream observations.
    entries: Vec<Entry>,
}

/// Pinned upstream source provenance.
#[derive(Debug, Clone, Deserialize)]
struct Upstream {
    /// Official repository URL.
    repository: String,
    /// Release tag used for source paths.
    tag: String,
    /// Full immutable commit identity.
    revision: String,
    /// Whether product execution depends on the checkout.
    product_dependency: bool,
}

/// One classified upstream concurrency observation.
#[derive(Debug, Clone, Deserialize)]
struct Entry {
    /// Stable inventory identity.
    id: String,
    /// Required MC-1 domain.
    domain: String,
    /// Path inside the pinned OTP repository.
    upstream_path: String,
    /// Function, structure, or suite area that carries the observation.
    upstream_anchor: String,
    /// MC-1 disposition class.
    classification: String,
    /// Terlan-relevant invariant or explicit rejection statement.
    invariant: String,
    /// Terlan contract clauses owning retained behavior.
    contract_ids: Vec<String>,
    /// Planned Terlan test identity for retained behavior.
    test_identity: Option<String>,
    /// Planned Terlan gate for retained behavior.
    planned_gate: Option<String>,
    /// Whether product execution depends on this upstream row.
    product_dependency: bool,
    /// Whether the observation is only ERTS machinery.
    implementation_detail: bool,
}

/// Success summary produced by the MC-1 inventory gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MulticoreInvariantInventorySummary {
    /// Number of classified rows.
    pub entry_count: usize,
    /// Number of required domains represented.
    pub domain_count: usize,
    /// Counts grouped by classification.
    pub classification_counts: BTreeMap<String, usize>,
}

/// Validates the checked-in MC-1 inventory and frozen Terlan contract.
///
/// Inputs:
/// - `root`: repository root containing `docs/runtime`.
///
/// Output:
/// - Inventory counts when every provenance, ownership, and contract mapping is
///   complete.
///
/// Transformation:
/// - Treats OTP paths as pinned citations while rejecting product dependencies
///   and retained invariants without Terlan owners.
pub fn run_multicore_invariant_inventory(
    root: &Path,
) -> QualityResult<MulticoreInvariantInventorySummary> {
    let inventory_text = read(root, INVENTORY_PATH)?;
    let contract_text = read(root, CONTRACT_PATH)?;
    let inventory = parse_inventory(&inventory_text)?;
    let diagnostics = validate_inventory(&inventory, &contract_text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(summary(&inventory))
}

/// Reads one required repository text file.
fn read(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|error| format!("{relative}: failed to read: {error}"))
}

/// Parses the versioned JSON inventory.
fn parse_inventory(text: &str) -> QualityResult<Inventory> {
    serde_json::from_str(text).map_err(|error| {
        format!("[vm-multicore-invariant-inventory] invalid JSON inventory: {error}")
    })
}

/// Validates provenance, coverage, classification, and Terlan ownership.
fn validate_inventory(inventory: &Inventory, contract: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    validate_provenance(inventory, &mut diagnostics);
    validate_contract(contract, &mut diagnostics);
    validate_domains(inventory, &mut diagnostics);
    validate_entries(inventory, contract, &mut diagnostics);
    diagnostics
}

/// Validates the frozen upstream and contract-document identities.
fn validate_provenance(inventory: &Inventory, diagnostics: &mut Vec<String>) {
    if inventory.schema != SCHEMA {
        diagnostics.push(format!("schema `{}` must be `{SCHEMA}`", inventory.schema));
    }
    if inventory.upstream.repository != OTP_REPOSITORY {
        diagnostics.push("upstream repository must be the official erlang/otp repository".into());
    }
    if inventory.upstream.tag != OTP_TAG {
        diagnostics.push(format!("upstream tag must be `{OTP_TAG}`"));
    }
    if inventory.upstream.revision != OTP_REVISION || !is_revision(&inventory.upstream.revision) {
        diagnostics.push(format!(
            "upstream revision must be the full pinned `{OTP_REVISION}` commit"
        ));
    }
    if inventory.upstream.product_dependency {
        diagnostics.push("the pinned OTP checkout cannot be a product dependency".into());
    }
    if inventory.contract_document != CONTRACT_PATH {
        diagnostics.push(format!("contract document must be `{CONTRACT_PATH}`"));
    }
}

/// Validates that one value is a full lowercase Git commit identity.
fn is_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Validates all frozen contract clauses and normative terms are present.
fn validate_contract(contract: &str, diagnostics: &mut Vec<String>) {
    for id in CONTRACT_IDS {
        if !contract.contains(&format!("## {id} ")) {
            diagnostics.push(format!("concurrency contract omits clause `{id}`"));
        }
    }
    let normalized = contract.split_whitespace().collect::<Vec<_>>().join(" ");
    for term in [
        "created -> runnable -> queued -> executing",
        "actor generation, lifecycle state, scheduler",
        "release publication",
        "matching acquire",
        "directory lookup guard",
        "Exactly one successful transition owns enqueue",
        "no directory reader can resolve its cell",
        "fails the execution shard closed",
        "Stable replay identities",
    ] {
        if !normalized.contains(term) {
            diagnostics.push(format!("concurrency contract omits frozen term `{term}`"));
        }
    }
}

/// Validates exact required-domain declaration and entry coverage.
fn validate_domains(inventory: &Inventory, diagnostics: &mut Vec<String>) {
    let declared = inventory
        .required_domains
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let required = REQUIRED_DOMAINS.iter().copied().collect::<BTreeSet<_>>();
    if declared != required || inventory.required_domains.len() != required.len() {
        diagnostics.push("required_domains must contain each MC-1 domain exactly once".into());
    }
    let covered = inventory
        .entries
        .iter()
        .map(|entry| entry.domain.as_str())
        .collect::<BTreeSet<_>>();
    for domain in required.difference(&covered) {
        diagnostics.push(format!(
            "inventory has no row for required domain `{domain}`"
        ));
    }
    for domain in covered.difference(&required) {
        diagnostics.push(format!("inventory row uses unknown domain `{domain}`"));
    }
}

/// Validates every classified observation and retained Terlan mapping.
fn validate_entries(inventory: &Inventory, contract: &str, diagnostics: &mut Vec<String>) {
    let mut ids = BTreeSet::new();
    let mut classification_counts = BTreeMap::<&str, usize>::new();
    for entry in &inventory.entries {
        if entry.id.trim().is_empty() || !ids.insert(entry.id.as_str()) {
            diagnostics.push(format!("duplicate or empty inventory id `{}`", entry.id));
        }
        if !CLASSIFICATIONS.contains(&entry.classification.as_str()) {
            diagnostics.push(format!(
                "entry `{}` has invalid classification `{}`",
                entry.id, entry.classification
            ));
            continue;
        }
        *classification_counts
            .entry(entry.classification.as_str())
            .or_default() += 1;
        validate_source(entry, diagnostics);
        validate_disposition(entry, contract, diagnostics);
    }
    for classification in CLASSIFICATIONS {
        if !classification_counts.contains_key(classification) {
            diagnostics.push(format!(
                "inventory does not exercise classification `{classification}`"
            ));
        }
    }
}

/// Validates one upstream citation remains non-executable product metadata.
fn validate_source(entry: &Entry, diagnostics: &mut Vec<String>) {
    let path = Path::new(&entry.upstream_path);
    if entry.upstream_path.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        diagnostics.push(format!("entry `{}` has invalid upstream path", entry.id));
    }
    if entry.upstream_anchor.trim().is_empty() {
        diagnostics.push(format!("entry `{}` has no upstream anchor", entry.id));
    }
    if entry.product_dependency {
        diagnostics.push(format!(
            "entry `{}` treats a deleted OTP path as a product dependency",
            entry.id
        ));
    }
}

/// Validates retained rows have owners and ERTS machinery is removed.
fn validate_disposition(entry: &Entry, contract: &str, diagnostics: &mut Vec<String>) {
    let removed = entry.classification == "remove-erts-implementation-detail";
    if entry.implementation_detail && !removed {
        diagnostics.push(format!(
            "entry `{}` mislabels ERTS mechanics as Terlan semantics",
            entry.id
        ));
    }
    if removed {
        if !entry.implementation_detail {
            diagnostics.push(format!(
                "removed entry `{}` must identify an ERTS implementation detail",
                entry.id
            ));
        }
        if !entry.contract_ids.is_empty()
            || nonempty(&entry.test_identity).is_some()
            || nonempty(&entry.planned_gate).is_some()
        {
            diagnostics.push(format!(
                "removed entry `{}` cannot own Terlan contract or gate mappings",
                entry.id
            ));
        }
        return;
    }

    if entry.invariant.trim().is_empty() {
        diagnostics.push(format!("retained entry `{}` has no invariant", entry.id));
    }
    if entry.contract_ids.is_empty() {
        diagnostics.push(format!(
            "retained entry `{}` has no contract owner",
            entry.id
        ));
    }
    for contract_id in &entry.contract_ids {
        if !CONTRACT_IDS.contains(&contract_id.as_str())
            || !contract.contains(&format!("## {contract_id} "))
        {
            diagnostics.push(format!(
                "retained entry `{}` references unknown contract `{contract_id}`",
                entry.id
            ));
        }
    }
    if nonempty(&entry.test_identity).is_none() {
        diagnostics.push(format!(
            "retained entry `{}` has no test identity",
            entry.id
        ));
    }
    if nonempty(&entry.planned_gate).is_none() {
        diagnostics.push(format!("retained entry `{}` has no planned gate", entry.id));
    }
}

/// Returns one nonempty optional string.
fn nonempty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.trim().is_empty())
}

/// Builds a deterministic successful inventory summary.
fn summary(inventory: &Inventory) -> MulticoreInvariantInventorySummary {
    let mut classification_counts = BTreeMap::new();
    for entry in &inventory.entries {
        *classification_counts
            .entry(entry.classification.clone())
            .or_default() += 1;
    }
    MulticoreInvariantInventorySummary {
        entry_count: inventory.entries.len(),
        domain_count: inventory.required_domains.len(),
        classification_counts,
    }
}

/// Renders stable quality diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-multicore-invariant-inventory] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "multicore_invariant_inventory_test.rs"]
#[cfg(test)]
mod multicore_invariant_inventory_test;
