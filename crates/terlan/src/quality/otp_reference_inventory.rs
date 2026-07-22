use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the OTP reference inventory.
const OTP_REFERENCE_INVENTORY_DOC: &str = "docs/runtime/OTP_REFERENCE_INVENTORY.md";

/// Ownership category assigned to one OTP reference entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OtpReferenceOwnership {
    CompilerOwned,
    VmOwned,
    BoundaryOwned,
    ReferenceOnly,
    OutOfContract,
}

/// Extraction status for one OTP reference entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OtpReferenceStatus {
    Mined,
    Pending,
    Rejected,
}

/// One retained OTP/Erlang/BEAM reference entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OtpReferenceEntry {
    id: &'static str,
    ownership: OtpReferenceOwnership,
    capability: &'static str,
    status: OtpReferenceStatus,
    unsupported_diagnostic: Option<&'static str>,
}

/// One parsed Markdown inventory row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OtpReferenceDocRow {
    id: String,
    source: String,
    ownership: String,
    capability: String,
    status: String,
}

/// Summary produced by the OTP reference inventory gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpReferenceInventorySummary {
    pub entry_count: usize,
    pub mined_count: usize,
    pub pending_count: usize,
    pub rejected_count: usize,
}

const INVENTORY: &[OtpReferenceEntry] = &[
    OtpReferenceEntry {
        id: "otp-pure-arithmetic",
        ownership: OtpReferenceOwnership::CompilerOwned,
        capability: "pure arithmetic lowering outside the VM when safe",
        status: OtpReferenceStatus::Pending,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-loader-literals",
        ownership: OtpReferenceOwnership::VmOwned,
        capability: "VM artifact loading and literal decoding",
        status: OtpReferenceStatus::Mined,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-send-receive",
        ownership: OtpReferenceOwnership::VmOwned,
        capability: "Terlan process message delivery",
        status: OtpReferenceStatus::Pending,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-selective-receive",
        ownership: OtpReferenceOwnership::VmOwned,
        capability: "Terlan selective receive cursor semantics",
        status: OtpReferenceStatus::Pending,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-timer-timeout",
        ownership: OtpReferenceOwnership::VmOwned,
        capability: "Terlan process timers and timeouts",
        status: OtpReferenceStatus::Pending,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-supervision-exit",
        ownership: OtpReferenceOwnership::VmOwned,
        capability: "Terlan supervision and failure propagation",
        status: OtpReferenceStatus::Pending,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-port-io",
        ownership: OtpReferenceOwnership::BoundaryOwned,
        capability: "typed host resource and IO boundary behavior",
        status: OtpReferenceStatus::Pending,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-http-baseline",
        ownership: OtpReferenceOwnership::ReferenceOnly,
        capability: "pre-removal HTTP runtime performance baseline",
        status: OtpReferenceStatus::Mined,
        unsupported_diagnostic: None,
    },
    OtpReferenceEntry {
        id: "otp-nif-abi",
        ownership: OtpReferenceOwnership::OutOfContract,
        capability: "native boundary rejects NIF ABI compatibility",
        status: OtpReferenceStatus::Rejected,
        unsupported_diagnostic: Some(
            "error[unsupported_capability]: native boundary rejects NIF ABI compatibility",
        ),
    },
];

const REQUIRED_TERMS: &[&str] = &[
    "not compatibility gates",
    "terlan capability",
    "active corpus fixtures",
    "compiler-owned",
    "vm-owned",
    "boundary-owned",
    "reference-only",
    "out-of-contract",
    "unsupported capability",
    "unsupported corpus rejections",
    "error[unsupported_capability]",
    "reference compiler/oracle",
    "not as the default runtime path",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "otp is a supported runtime target",
    "beam compatibility is required",
    "otp compatibility gate",
    "erlc is the default runtime path",
    "stock otp is the default runtime path",
];

const ACTIVE_CORPUS_FIXTURE_IDS: &[&str] = &["otp-loader-literals"];

/// Runs the OTP reference inventory gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/runtime/`.
///
/// Output:
/// - Counts for retained reference entries and their extraction status.
/// - Stable diagnostics if the inventory document is missing required terms,
///   omits an entry, or treats OTP material as compatibility scope.
///
/// Transformation:
/// - Validates the checked-in inventory document against the typed inventory
///   table used by 0.0.7 runtime migration gates.
pub fn run_otp_reference_inventory(root: &Path) -> QualityResult<OtpReferenceInventorySummary> {
    let path = root.join(OTP_REFERENCE_INVENTORY_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read OTP reference inventory: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_inventory_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(summary())
}

/// Validates the reference inventory document.
fn validate_inventory_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(term) {
            diagnostics.push(format!("missing OTP reference inventory term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden OTP reference inventory claim `{claim}`"));
        }
    }
    for entry in INVENTORY {
        if !normalized.contains(entry.id) {
            diagnostics.push(format!("missing OTP reference entry `{}`", entry.id));
        }
        if entry.capability.trim().is_empty() {
            diagnostics.push(format!(
                "OTP reference entry `{}` has no Terlan capability",
                entry.id
            ));
        }
    }
    diagnostics.extend(validate_markdown_inventory_rows(text));
    diagnostics.extend(validate_active_corpus_fixture_mappings(
        INVENTORY,
        ACTIVE_CORPUS_FIXTURE_IDS,
    ));
    diagnostics.extend(validate_unsupported_diagnostics(text, INVENTORY));
    diagnostics.extend(validate_status_ownership());
    diagnostics
}

/// Validates the checked-in Markdown table matches the typed inventory.
fn validate_markdown_inventory_rows(text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let rows = parse_markdown_inventory_rows(text, &mut diagnostics);
    for entry in INVENTORY {
        let Some(row) = rows.iter().find(|row| row.id == entry.id) else {
            diagnostics.push(format!("missing Markdown OTP reference row `{}`", entry.id));
            continue;
        };
        if row.source.trim().is_empty() {
            diagnostics.push(format!(
                "Markdown OTP reference row `{}` has no source",
                entry.id
            ));
        }
        if row.capability.trim().is_empty() {
            diagnostics.push(format!(
                "Markdown OTP reference row `{}` has no Terlan capability",
                entry.id
            ));
        }
        if row.ownership != entry.ownership.as_str() {
            diagnostics.push(format!(
                "Markdown OTP reference row `{}` ownership `{}` must be `{}`",
                entry.id,
                row.ownership,
                entry.ownership.as_str()
            ));
        }
        if row.capability != entry.capability {
            diagnostics.push(format!(
                "Markdown OTP reference row `{}` capability `{}` must be `{}`",
                entry.id, row.capability, entry.capability
            ));
        }
        if row.status != entry.status.as_str() {
            diagnostics.push(format!(
                "Markdown OTP reference row `{}` status `{}` must be `{}`",
                entry.id,
                row.status,
                entry.status.as_str()
            ));
        }
    }
    for row in rows {
        if !INVENTORY.iter().any(|entry| entry.id == row.id) {
            diagnostics.push(format!("unknown Markdown OTP reference row `{}`", row.id));
        }
    }
    diagnostics
}

/// Parses the Markdown inventory table into rows.
fn parse_markdown_inventory_rows(
    text: &str,
    diagnostics: &mut Vec<String>,
) -> Vec<OtpReferenceDocRow> {
    let mut rows = Vec::new();
    let mut in_entries_section = !text.contains("## Entries");
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_entries_section = trimmed == "## Entries";
            continue;
        }
        if !in_entries_section {
            continue;
        }
        if !trimmed.starts_with('|') {
            continue;
        }
        if trimmed.contains(" Id ") || trimmed.contains("---") {
            continue;
        }
        let columns = trimmed
            .trim_matches('|')
            .split('|')
            .map(|column| column.trim().to_owned())
            .collect::<Vec<_>>();
        if columns.len() != 5 {
            diagnostics.push(format!(
                "OTP reference inventory row `{trimmed}` must have 5 columns"
            ));
            continue;
        }
        rows.push(OtpReferenceDocRow {
            id: columns[0].clone(),
            source: columns[1].clone(),
            ownership: columns[2].clone(),
            capability: columns[3].clone(),
            status: columns[4].clone(),
        });
    }
    rows
}

/// Validates status and ownership combinations that would weaken the contract.
fn validate_status_ownership() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for entry in INVENTORY {
        if entry.status == OtpReferenceStatus::Rejected
            && entry.ownership != OtpReferenceOwnership::OutOfContract
        {
            diagnostics.push(format!(
                "rejected OTP entry `{}` must be out-of-contract",
                entry.id
            ));
        }
        if entry.ownership == OtpReferenceOwnership::OutOfContract
            && entry.status != OtpReferenceStatus::Rejected
        {
            diagnostics.push(format!(
                "out-of-contract OTP entry `{}` must be rejected",
                entry.id
            ));
        }
    }
    diagnostics
}

/// Validates out-of-contract entries have typed unsupported-capability output.
fn validate_unsupported_diagnostics(text: &str, entries: &[OtpReferenceEntry]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for entry in entries {
        match (entry.ownership, entry.unsupported_diagnostic) {
            (OtpReferenceOwnership::OutOfContract, Some(diagnostic)) => {
                if !diagnostic.starts_with("error[unsupported_capability]:") {
                    diagnostics.push(format!(
                        "out-of-contract OTP entry `{}` must use error[unsupported_capability]",
                        entry.id
                    ));
                }
                if !text.contains(diagnostic) {
                    diagnostics.push(format!(
                        "out-of-contract OTP entry `{}` diagnostic is missing from the Markdown inventory",
                        entry.id
                    ));
                }
            }
            (OtpReferenceOwnership::OutOfContract, None) => {
                diagnostics.push(format!(
                    "out-of-contract OTP entry `{}` has no unsupported-capability diagnostic",
                    entry.id
                ));
            }
            (_, Some(_)) => {
                diagnostics.push(format!(
                    "OTP entry `{}` must not declare unsupported-capability diagnostics unless it is out-of-contract",
                    entry.id
                ));
            }
            (_, None) => {}
        }
    }
    diagnostics
}

/// Validates active corpus fixtures map to Terlan-owned capability categories.
fn validate_active_corpus_fixture_mappings(
    entries: &[OtpReferenceEntry],
    active_ids: &[&str],
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for active_id in active_ids {
        let Some(entry) = entries.iter().find(|entry| entry.id == *active_id) else {
            diagnostics.push(format!(
                "active corpus fixture `{active_id}` is not inventoried"
            ));
            continue;
        };
        if entry.capability.trim().is_empty() {
            diagnostics.push(format!(
                "active corpus fixture `{active_id}` has no Terlan capability"
            ));
        }
        if !entry.ownership.is_active_corpus_allowed() {
            diagnostics.push(format!(
                "active corpus fixture `{active_id}` must be compiler-owned, vm-owned, or boundary-owned; found `{}`",
                entry.ownership.as_str()
            ));
        }
    }
    diagnostics
}

impl OtpReferenceOwnership {
    /// Returns the canonical Markdown spelling for the ownership category.
    fn as_str(self) -> &'static str {
        match self {
            OtpReferenceOwnership::CompilerOwned => "compiler-owned",
            OtpReferenceOwnership::VmOwned => "vm-owned",
            OtpReferenceOwnership::BoundaryOwned => "boundary-owned",
            OtpReferenceOwnership::ReferenceOnly => "reference-only",
            OtpReferenceOwnership::OutOfContract => "out-of-contract",
        }
    }

    /// Returns whether this category may back an active corpus fixture.
    fn is_active_corpus_allowed(self) -> bool {
        matches!(
            self,
            OtpReferenceOwnership::CompilerOwned
                | OtpReferenceOwnership::VmOwned
                | OtpReferenceOwnership::BoundaryOwned
        )
    }
}

impl OtpReferenceStatus {
    /// Returns the canonical Markdown spelling for the extraction status.
    fn as_str(self) -> &'static str {
        match self {
            OtpReferenceStatus::Mined => "mined",
            OtpReferenceStatus::Pending => "pending",
            OtpReferenceStatus::Rejected => "rejected",
        }
    }
}

/// Builds the success summary.
fn summary() -> OtpReferenceInventorySummary {
    let mut summary = OtpReferenceInventorySummary {
        entry_count: INVENTORY.len(),
        mined_count: 0,
        pending_count: 0,
        rejected_count: 0,
    };
    for entry in INVENTORY {
        match entry.status {
            OtpReferenceStatus::Mined => summary.mined_count += 1,
            OtpReferenceStatus::Pending => summary.pending_count += 1,
            OtpReferenceStatus::Rejected => summary.rejected_count += 1,
        }
    }
    summary
}

/// Renders inventory diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[otp-reference-inventory] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "otp_reference_inventory_test.rs"]
mod otp_reference_inventory_test;
