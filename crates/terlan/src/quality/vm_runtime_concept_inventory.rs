use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the Terlan VM runtime concept inventory.
const VM_RUNTIME_CONCEPTS_DOC: &str = "docs/runtime/TERLAN_VM_RUNTIME_CONCEPTS.md";

/// Runtime concept classifications used by the 0.0.7 VM expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmRuntimeConceptClassification {
    RequiredVmSemantics,
    LibraryAbstraction,
    DistributionMachinery,
    UnsupportedOtpCompatibility,
}

/// One runtime concept classified for Terlan VM ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VmRuntimeConceptEntry {
    concept: &'static str,
    classification: VmRuntimeConceptClassification,
}

/// Summary produced by the VM concept inventory gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmRuntimeConceptInventorySummary {
    pub concept_count: usize,
    pub required_vm_semantics_count: usize,
    pub library_abstraction_count: usize,
    pub distribution_machinery_count: usize,
    pub unsupported_otp_compatibility_count: usize,
}

const REQUIRED_TERMS: &[&str] = &[
    "required-vm-semantics",
    "library-abstraction",
    "distribution-machinery",
    "unsupported-otp-compatibility",
    "process identity",
    "scheduler reductions",
    "mailbox ordering",
    "selective receive",
    "local spawn",
    "local send",
    "self reference",
    "timers",
    "links",
    "monitors",
    "trapped exits",
    "supervisor trees",
    "resource ownership",
    "heap pressure",
    "hot reload generations",
    "VM inspection",
    "VM-owned table storage",
    "node identity",
    "distributed envelopes",
    "cluster capability checks",
    "network partition simulation",
    "BEAM opcode parity",
    "arbitrary OTP application boot",
    "ERTS packaging compatibility",
    "dynamic atom creation",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "beam opcode parity is required",
    "arbitrary otp applications are supported",
    "erts packaging compatibility is required",
    "dynamic atom creation is allowed",
];

const INVENTORY: &[VmRuntimeConceptEntry] = &[
    required("process identity"),
    required("scheduler reductions"),
    required("mailbox ordering"),
    required("selective receive"),
    required("local spawn"),
    required("local send"),
    required("self reference"),
    required("timers"),
    required("links"),
    required("monitors"),
    required("trapped exits"),
    required("supervisor trees"),
    required("resource ownership"),
    required("heap pressure"),
    required("hot reload generations"),
    required("VM inspection"),
    library("VM-owned table storage"),
    library("task abstraction"),
    library("agent abstraction"),
    library("gen-server abstraction"),
    distribution("node identity"),
    distribution("distributed envelopes"),
    distribution("cluster capability checks"),
    distribution("network partition simulation"),
    unsupported("BEAM opcode parity"),
    unsupported("arbitrary OTP application boot"),
    unsupported("ERTS packaging compatibility"),
    unsupported("dynamic atom creation"),
];

/// Runs the VM runtime-concept inventory gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/runtime/`.
///
/// Output:
/// - Classification counts when the inventory document is coherent.
/// - Stable diagnostics when required concepts, classifications, or rejection
///   rules drift.
///
/// Transformation:
/// - Validates the checked-in concept inventory and mirrors it against the
///   Rust-side release gate inventory so roadmap requirements stay executable.
pub fn run_vm_runtime_concept_inventory(
    root: &Path,
) -> QualityResult<VmRuntimeConceptInventorySummary> {
    let mut diagnostics = validate_inventory_shape();
    diagnostics.extend(validate_document(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(summary())
}

/// Builds a required VM-semantics inventory row.
const fn required(concept: &'static str) -> VmRuntimeConceptEntry {
    VmRuntimeConceptEntry {
        concept,
        classification: VmRuntimeConceptClassification::RequiredVmSemantics,
    }
}

/// Builds a library-abstraction inventory row.
const fn library(concept: &'static str) -> VmRuntimeConceptEntry {
    VmRuntimeConceptEntry {
        concept,
        classification: VmRuntimeConceptClassification::LibraryAbstraction,
    }
}

/// Builds a distribution-machinery inventory row.
const fn distribution(concept: &'static str) -> VmRuntimeConceptEntry {
    VmRuntimeConceptEntry {
        concept,
        classification: VmRuntimeConceptClassification::DistributionMachinery,
    }
}

/// Builds an unsupported-OTP-compatibility inventory row.
const fn unsupported(concept: &'static str) -> VmRuntimeConceptEntry {
    VmRuntimeConceptEntry {
        concept,
        classification: VmRuntimeConceptClassification::UnsupportedOtpCompatibility,
    }
}

/// Validates the Rust-side inventory shape.
fn validate_inventory_shape() -> Vec<String> {
    let summary = summary();
    let mut diagnostics = Vec::new();
    if summary.required_vm_semantics_count == 0 {
        diagnostics.push("VM concept inventory has no required VM semantics".to_string());
    }
    if summary.library_abstraction_count == 0 {
        diagnostics.push("VM concept inventory has no library abstractions".to_string());
    }
    if summary.distribution_machinery_count == 0 {
        diagnostics.push("VM concept inventory has no distribution machinery".to_string());
    }
    if summary.unsupported_otp_compatibility_count == 0 {
        diagnostics
            .push("VM concept inventory has no unsupported OTP compatibility rows".to_string());
    }
    diagnostics
}

/// Validates the checked-in concept inventory document.
fn validate_document(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join(VM_RUNTIME_CONCEPTS_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read VM runtime concept inventory: {err}",
            path.display()
        )
    })?;
    Ok(validate_document_text(&text))
}

/// Validates concept inventory text.
fn validate_document_text(text: &str) -> Vec<String> {
    let normalized = normalize_text(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&normalize_text(term)) {
            diagnostics.push(format!("missing VM runtime concept term `{term}`"));
        }
    }
    for entry in INVENTORY {
        if !normalized.contains(&normalize_text(entry.concept)) {
            diagnostics.push(format!(
                "missing VM runtime concept inventory row `{}`",
                entry.concept
            ));
        }
        if !normalized.contains(classification_name(entry.classification)) {
            diagnostics.push(format!(
                "missing VM runtime concept classification `{}`",
                classification_name(entry.classification)
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&normalize_text(claim)) {
            diagnostics.push(format!("forbidden VM runtime concept claim `{claim}`"));
        }
    }
    diagnostics
}

/// Returns the document spelling for a classification.
const fn classification_name(classification: VmRuntimeConceptClassification) -> &'static str {
    match classification {
        VmRuntimeConceptClassification::RequiredVmSemantics => "required-vm-semantics",
        VmRuntimeConceptClassification::LibraryAbstraction => "library-abstraction",
        VmRuntimeConceptClassification::DistributionMachinery => "distribution-machinery",
        VmRuntimeConceptClassification::UnsupportedOtpCompatibility => {
            "unsupported-otp-compatibility"
        }
    }
}

/// Normalizes prose for stable term matching across line wrapping.
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds the success summary.
fn summary() -> VmRuntimeConceptInventorySummary {
    let mut summary = VmRuntimeConceptInventorySummary {
        concept_count: INVENTORY.len(),
        required_vm_semantics_count: 0,
        library_abstraction_count: 0,
        distribution_machinery_count: 0,
        unsupported_otp_compatibility_count: 0,
    };
    for entry in INVENTORY {
        match entry.classification {
            VmRuntimeConceptClassification::RequiredVmSemantics => {
                summary.required_vm_semantics_count += 1;
            }
            VmRuntimeConceptClassification::LibraryAbstraction => {
                summary.library_abstraction_count += 1;
            }
            VmRuntimeConceptClassification::DistributionMachinery => {
                summary.distribution_machinery_count += 1;
            }
            VmRuntimeConceptClassification::UnsupportedOtpCompatibility => {
                summary.unsupported_otp_compatibility_count += 1;
            }
        }
    }
    summary
}

/// Renders inventory diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-runtime-concept-inventory] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_runtime_concept_inventory_test.rs"]
mod vm_runtime_concept_inventory_test;
