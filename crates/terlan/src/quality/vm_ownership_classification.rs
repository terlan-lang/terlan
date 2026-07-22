use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the Terlan VM ownership contract.
const VM_OWNERSHIP_DOC: &str = "docs/runtime/TERLAN_VM_OWNERSHIP.md";

/// Runtime ownership categories used by the 0.0.7 VM transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmOwnershipCategory {
    CompilerOwned,
    VmOwned,
    BoundaryOwned,
    OutOfContract,
}

/// One source path covered by the ownership inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VmOwnershipEntry {
    path: &'static str,
    category: VmOwnershipCategory,
    capability: &'static str,
}

/// Summary produced by the VM ownership classification gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmOwnershipClassificationSummary {
    pub inventory_count: usize,
    pub compiler_owned_count: usize,
    pub vm_owned_count: usize,
    pub boundary_owned_count: usize,
    pub reference_only_count: usize,
    pub out_of_contract_count: usize,
}

const REQUIRED_TERMS: &[&str] = &[
    "compiler-owned",
    "vm-owned",
    "boundary-owned",
    "reference-only",
    "out-of-contract",
    "vm-owned behavior is limited to runtime semantics",
    "process identity",
    "message passing",
    "scheduler reductions",
    "native modules",
    "filesystem",
    "mobile shell bridges",
    "wasi workers",
    "native-owned pure/static code stays compiler-owned",
    "reliable primitives for distributed algorithms, not the algorithms themselves",
    "durable append/log hooks",
    "fencing support",
    "consensus protocols",
    "replication policies",
    "crdts",
    "host capability validation",
    "unsupported-capability diagnostic",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "otp compatibility is the runtime contract",
    "beam opcode parity is required",
    "nif abi compatibility is required",
    "paxos is vm-owned",
    "vsr is vm-owned",
    "the vm owns consensus protocols",
    "the vm provides paxos",
    "the vm provides vsr",
];

const INVENTORY: &[VmOwnershipEntry] = &[
    VmOwnershipEntry {
        path: "crates/terlan/src/commands/build/vm_artifact.rs",
        category: VmOwnershipCategory::CompilerOwned,
        capability: "VM artifact emission",
    },
    VmOwnershipEntry {
        path: "crates/terlan/src/commands/emit_js",
        category: VmOwnershipCategory::CompilerOwned,
        capability: "target-specific pure code emission",
    },
    VmOwnershipEntry {
        path: "crates/terlan/src/vm",
        category: VmOwnershipCategory::VmOwned,
        capability: "Terlan runtime execution and artifact loading",
    },
    VmOwnershipEntry {
        path: "crates/terlan/src/commands/serve",
        category: VmOwnershipCategory::BoundaryOwned,
        capability: "HTTP host boundary",
    },
    VmOwnershipEntry {
        path: "crates/terlan/src/commands/db",
        category: VmOwnershipCategory::BoundaryOwned,
        capability: "Postgres host boundary",
    },
    VmOwnershipEntry {
        path: "crates/terlan/src/commands/native_vector_runtime.rs",
        category: VmOwnershipCategory::BoundaryOwned,
        capability: "native resource boundary",
    },
    VmOwnershipEntry {
        path: "docs/runtime/TERLAN_VM_OWNERSHIP.md",
        category: VmOwnershipCategory::OutOfContract,
        capability: "documents behavior intentionally rejected by the default runtime",
    },
];

/// Runs the VM ownership classification gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/runtime/` and `crates/terlan/`.
///
/// Output:
/// - Category counts when the ownership contract and inventory are coherent.
/// - Stable diagnostics when contract language is missing or an inventory path
///   no longer exists.
///
/// Transformation:
/// - Validates the ownership contract document, then verifies each retained
///   runtime path has a product capability and an ownership category.
pub fn run_vm_ownership_classification(
    root: &Path,
) -> QualityResult<VmOwnershipClassificationSummary> {
    let mut diagnostics = validate_contract_doc(root)?;
    diagnostics.extend(validate_inventory(root));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(summary())
}

/// Validates the ownership contract document.
fn validate_contract_doc(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join(VM_OWNERSHIP_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read VM ownership contract: {err}",
            path.display()
        )
    })?;
    Ok(validate_contract_text(&text))
}

/// Validates ownership contract text.
fn validate_contract_text(text: &str) -> Vec<String> {
    let normalized = normalize_contract_text(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&normalize_contract_text(term)) {
            diagnostics.push(format!("missing VM ownership contract term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&normalize_contract_text(claim)) {
            diagnostics.push(format!("forbidden VM ownership claim `{claim}`"));
        }
    }
    diagnostics
}

/// Normalizes prose for stable term matching across line wrapping.
fn normalize_contract_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Validates inventory paths and capability labels.
fn validate_inventory(root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for entry in INVENTORY {
        if entry.capability.trim().is_empty() {
            diagnostics.push(format!("inventory path `{}` has no capability", entry.path));
        }
        if !root.join(entry.path).exists() {
            diagnostics.push(format!("inventory path `{}` is missing", entry.path));
        }
    }
    diagnostics
}

/// Builds the success summary.
fn summary() -> VmOwnershipClassificationSummary {
    let mut summary = VmOwnershipClassificationSummary {
        inventory_count: INVENTORY.len(),
        compiler_owned_count: 0,
        vm_owned_count: 0,
        boundary_owned_count: 0,
        reference_only_count: 0,
        out_of_contract_count: 0,
    };
    for entry in INVENTORY {
        match entry.category {
            VmOwnershipCategory::CompilerOwned => summary.compiler_owned_count += 1,
            VmOwnershipCategory::VmOwned => summary.vm_owned_count += 1,
            VmOwnershipCategory::BoundaryOwned => summary.boundary_owned_count += 1,
            VmOwnershipCategory::OutOfContract => summary.out_of_contract_count += 1,
        }
    }
    summary
}

/// Renders ownership diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-ownership-classification] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_ownership_classification_test.rs"]
mod vm_ownership_classification_test;
