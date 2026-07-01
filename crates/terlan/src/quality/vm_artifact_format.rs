use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the Terlan VM artifact contract.
const VM_ARTIFACT_FORMAT_DOC: &str = "docs/runtime/TERLAN_VM_ARTIFACT_FORMAT.md";

/// Required semantic groups for the VM artifact contract.
const REQUIRED_GROUPS: &[RequiredGroup] = &[
    RequiredGroup::single("title", "terlan vm artifact format"),
    RequiredGroup::single("0.0.7 baseline", "0.0.7 baseline"),
    RequiredGroup::any("CoreIR derivation", &["coreir", "runtime ir"]),
    RequiredGroup::single("not Erlang source", "not erlang source"),
    RequiredGroup::single("not BEAM bytecode", "not beam bytecode"),
    RequiredGroup::single("not NIF ABI", "not a nif abi"),
    RequiredGroup::single("deterministic artifact", "deterministic"),
    RequiredGroup::single("schema version", "schema_version"),
    RequiredGroup::single("artifact kind", "artifact_kind"),
    RequiredGroup::single("compiler version", "compiler_version"),
    RequiredGroup::single("target profile", "target_profile"),
    RequiredGroup::single("module records", "module"),
    RequiredGroup::single("exports", "exports"),
    RequiredGroup::single("functions", "functions"),
    RequiredGroup::single("types", "types"),
    RequiredGroup::single("constants", "constants"),
    RequiredGroup::single("capabilities", "capabilities"),
    RequiredGroup::single("native boundary", "native_boundary"),
    RequiredGroup::single("source maps", "source_map"),
    RequiredGroup::single("debug metadata", "debug"),
    RequiredGroup::single("checksum", "checksum"),
    RequiredGroup::single("validation", "validation"),
    RequiredGroup::single("non-goals", "non-goals"),
];

/// Forbidden claims that would make the artifact contract drift back to BEAM.
const FORBIDDEN_DEFAULT_CLAIMS: &[&str] = &[
    "default artifact is beam",
    "default vm artifact is beam",
    "beam bytecode is the default",
    "generated erlang is the default",
    "erlang source is the default",
    "otp is the default runtime",
];

/// Summary produced by the VM artifact contract check.
///
/// Inputs:
/// - Number of semantic groups enforced by the check.
///
/// Output:
/// - Stable success metric for CI output.
///
/// Transformation:
/// - Separates the validation count from failure diagnostics so the command
///   wrapper can print compact success text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmArtifactFormatSummary {
    pub required_group_count: usize,
}

/// Required phrase group for the artifact contract.
///
/// Inputs:
/// - `label`: human-readable requirement name.
/// - `phrases`: accepted lowercase text fragments.
///
/// Output:
/// - Immutable rule used by the contract checker.
///
/// Transformation:
/// - Allows a requirement to accept one exact phrase or a small set of
///   equivalent phrases without making the documentation wording brittle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequiredGroup {
    label: &'static str,
    phrases: RequiredPhrases,
}

/// Accepted phrase shape for one required group.
///
/// Inputs:
/// - Single phrase or multiple equivalent phrases.
///
/// Output:
/// - Static phrase holder for validation.
///
/// Transformation:
/// - Avoids allocating or borrowing temporary slices for one-phrase groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredPhrases {
    Single(&'static str),
    Any(&'static [&'static str]),
}

impl RequiredGroup {
    /// Builds a required group with one accepted phrase.
    ///
    /// Inputs:
    /// - Requirement label.
    /// - Required lowercase phrase.
    ///
    /// Output:
    /// - Required group with one accepted phrase.
    ///
    /// Transformation:
    /// - Stores the phrase directly so validation can avoid temporary slices.
    const fn single(label: &'static str, phrase: &'static str) -> Self {
        Self {
            label,
            phrases: RequiredPhrases::Single(phrase),
        }
    }

    /// Builds a required group with multiple accepted phrases.
    ///
    /// Inputs:
    /// - Requirement label.
    /// - Accepted lowercase phrases.
    ///
    /// Output:
    /// - Required group using any-match semantics.
    ///
    /// Transformation:
    /// - Lets the contract use either CoreIR or runtime IR wording while still
    ///   requiring compiler-owned artifact derivation.
    const fn any(label: &'static str, phrases: &'static [&'static str]) -> Self {
        Self {
            label,
            phrases: RequiredPhrases::Any(phrases),
        }
    }

    /// Returns whether the normalized document satisfies this group.
    ///
    /// Inputs:
    /// - Lowercase document text.
    ///
    /// Output:
    /// - `true` when any accepted phrase is present.
    ///
    /// Transformation:
    /// - Applies simple substring matching for stable documentation gates.
    fn matches(&self, normalized_text: &str) -> bool {
        match self.phrases {
            RequiredPhrases::Single(phrase) => normalized_text.contains(phrase),
            RequiredPhrases::Any(phrases) => phrases
                .iter()
                .any(|phrase| normalized_text.contains(phrase)),
        }
    }
}

/// Runs the Terlan VM artifact contract check.
///
/// Inputs:
/// - `root`: repository root containing `docs/runtime/`.
///
/// Output:
/// - Success summary when the artifact contract is present and explicit.
/// - Stable diagnostics when required contract language is missing or drifts
///   back to Erlang/BEAM defaults.
///
/// Transformation:
/// - Reads the checked-in VM artifact contract and validates required semantic
///   groups plus forbidden default-runtime claims.
pub fn run_vm_artifact_format(root: &Path) -> QualityResult<VmArtifactFormatSummary> {
    let path = root.join(VM_ARTIFACT_FORMAT_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read VM artifact contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_vm_artifact_format_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_vm_artifact_format_failure(&diagnostics));
    }
    Ok(VmArtifactFormatSummary {
        required_group_count: REQUIRED_GROUPS.len(),
    })
}

/// Validates VM artifact contract text.
///
/// Inputs:
/// - `text`: documentation text.
///
/// Output:
/// - Diagnostics for missing required semantic groups or forbidden default
///   runtime claims.
///
/// Transformation:
/// - Lowercases the text, checks required groups, then checks forbidden
///   phrases that would reintroduce BEAM/Erlang as the default runtime.
fn validate_vm_artifact_format_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for group in REQUIRED_GROUPS {
        if !group.matches(&normalized) {
            diagnostics.push(format!(
                "{}: missing VM artifact contract language",
                group.label
            ));
        }
    }
    for claim in FORBIDDEN_DEFAULT_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden default-runtime claim: `{claim}`"));
        }
    }
    diagnostics
}

/// Renders VM artifact check diagnostics.
///
/// Inputs:
/// - `diagnostics`: individual validation failures.
///
/// Output:
/// - Stable multi-line failure block.
///
/// Transformation:
/// - Keeps Make and CI output readable without exposing internal data
///   structures.
fn render_vm_artifact_format_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-artifact-format] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_artifact_format_test.rs"]
mod vm_artifact_format_test;
