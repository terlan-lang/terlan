use super::*;

/// Verifies summary status counts stay coherent.
///
/// Inputs:
/// - Static OTP reference inventory.
///
/// Output:
/// - Test passes when status counts add up to the inventory size.
///
/// Transformation:
/// - Locks command output to the typed inventory table.
#[test]
fn summary_counts_cover_all_inventory_entries() {
    let summary = summary();
    assert_eq!(
        summary.entry_count,
        summary.mined_count + summary.pending_count + summary.rejected_count
    );
}

/// Verifies the inventory validator rejects compatibility claims.
///
/// Inputs:
/// - A compact inventory text with required terms and a forbidden claim.
///
/// Output:
/// - Test passes when the forbidden compatibility claim is reported.
///
/// Transformation:
/// - Ensures reference material cannot drift into supported OTP runtime scope.
#[test]
fn inventory_text_rejects_compatibility_claims() {
    let mut text = inventory_text_with_rows();
    text.push_str("\nOTP is a supported runtime target.");

    let diagnostics = validate_inventory_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("otp is a supported runtime target")),
        "diagnostics should reject supported-runtime claim: {diagnostics:?}"
    );
}

/// Verifies the inventory validator rejects `erlc` as a default runtime path.
///
/// Inputs:
/// - Valid inventory text plus a forbidden default-runtime claim.
///
/// Output:
/// - Test passes when the forbidden `erlc` runtime-path claim is reported.
///
/// Transformation:
/// - Keeps stock OTP/`erlc` constrained to reference/oracle or migration bridge
///   usage.
#[test]
fn inventory_text_rejects_erlc_default_runtime_claims() {
    let mut text = inventory_text_with_rows();
    text.push_str("\nerlc is the default runtime path.");

    let diagnostics = validate_inventory_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("erlc is the default runtime path")),
        "diagnostics should reject erlc default-runtime claim: {diagnostics:?}"
    );
}

/// Verifies status and ownership invariants remain strict.
///
/// Inputs:
/// - Static OTP reference inventory.
///
/// Output:
/// - Test passes when rejected entries are out-of-contract and vice versa.
///
/// Transformation:
/// - Protects the policy that unsupported OTP behavior is explicit rather than
///   left as pending compatibility work.
#[test]
fn status_ownership_rejects_non_contract_drift() {
    assert!(validate_status_ownership().is_empty());
}

/// Verifies out-of-contract entries require typed unsupported diagnostics.
///
/// Inputs:
/// - Static inventory and generated Markdown rejection rows.
///
/// Output:
/// - Test passes when the unsupported diagnostic contract is satisfied.
///
/// Transformation:
/// - Keeps rejected OTP behavior explicit and typed instead of silently falling
///   back to OTP execution.
#[test]
fn unsupported_diagnostics_accept_out_of_contract_entries() {
    assert!(validate_unsupported_diagnostics(&inventory_text_with_rows(), INVENTORY).is_empty());
}

/// Verifies out-of-contract entries fail without a documented diagnostic.
///
/// Inputs:
/// - Inventory text without the unsupported rejection table.
///
/// Output:
/// - Test passes when the missing diagnostic is reported.
///
/// Transformation:
/// - Prevents unsupported corpus cases from remaining as vague compatibility
///   gaps.
#[test]
fn unsupported_diagnostics_reject_missing_markdown_diagnostic() {
    let diagnostics = validate_unsupported_diagnostics(&markdown_inventory_rows(), INVENTORY);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("diagnostic is missing")),
        "diagnostics should reject missing unsupported diagnostic: {diagnostics:?}"
    );
}

/// Verifies active corpus fixtures accept Terlan-owned categories.
///
/// Inputs:
/// - Static OTP reference inventory and the active corpus fixture id list.
///
/// Output:
/// - Test passes when all active fixtures map to compiler, VM, or boundary
///   ownership.
///
/// Transformation:
/// - Keeps active corpus gates tied to Terlan product capability categories.
#[test]
fn active_corpus_fixture_mappings_accept_owned_entries() {
    assert!(
        validate_active_corpus_fixture_mappings(INVENTORY, ACTIVE_CORPUS_FIXTURE_IDS).is_empty()
    );
}

/// Verifies active corpus fixtures reject reference-only entries.
///
/// Inputs:
/// - Static OTP reference inventory with a reference-only entry treated as an
///   active corpus fixture.
///
/// Output:
/// - Test passes when the reference-only ownership is rejected.
///
/// Transformation:
/// - Prevents benchmark/reference material from becoming an active runtime gate
///   without reclassification.
#[test]
fn active_corpus_fixture_mappings_reject_reference_only_entries() {
    let diagnostics = validate_active_corpus_fixture_mappings(INVENTORY, &["otp-http-baseline"]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("must be compiler-owned")),
        "diagnostics should reject reference-only active fixtures: {diagnostics:?}"
    );
}

/// Verifies the Markdown table parser accepts the typed inventory rows.
///
/// Inputs:
/// - Generated Markdown table rows for every static inventory entry.
///
/// Output:
/// - Test passes when the table-level validator reports no diagnostics.
///
/// Transformation:
/// - Keeps the checked-in inventory document aligned with the typed gate.
#[test]
fn markdown_inventory_rows_accept_typed_inventory() {
    assert!(validate_markdown_inventory_rows(&inventory_text_with_rows()).is_empty());
}

/// Verifies Markdown inventory rows must name a Terlan capability.
///
/// Inputs:
/// - Inventory table where one retained row has an empty capability column.
///
/// Output:
/// - Test passes when the missing capability is reported.
///
/// Transformation:
/// - Prevents retained OTP/BEAM corpus entries from becoming unclassified
///   compatibility fixtures.
#[test]
fn markdown_inventory_rows_reject_missing_capability() {
    let text = inventory_text_with_custom_row(
        "otp-pure-arithmetic",
        "OTP arithmetic expression examples",
        "compiler-owned",
        "",
        "pending",
    );

    let diagnostics = validate_markdown_inventory_rows(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("has no Terlan capability")),
        "diagnostics should reject missing row capability: {diagnostics:?}"
    );
}

fn inventory_text_with_rows() -> String {
    let mut text = REQUIRED_TERMS.join("\n");
    text.push('\n');
    text.push_str("## Entries\n");
    text.push_str(&markdown_inventory_rows());
    text.push('\n');
    text.push_str("## Unsupported Corpus Rejections\n");
    text.push_str(&unsupported_rejection_rows());
    text
}

fn inventory_text_with_custom_row(
    id: &str,
    source: &str,
    ownership: &str,
    capability: &str,
    status: &str,
) -> String {
    let mut text =
        String::from("| Id | Source | Ownership | Terlan capability | Extraction status |\n");
    text.push_str("| --- | --- | --- | --- | --- |\n");
    text.push_str(&format!(
        "| {id} | {source} | {ownership} | {capability} | {status} |\n"
    ));
    for entry in INVENTORY {
        if entry.id == id {
            continue;
        }
        text.push_str(&format!(
            "| {} | source | {} | {} | {} |\n",
            entry.id,
            entry.ownership.as_str(),
            entry.capability,
            entry.status.as_str()
        ));
    }
    text.push('\n');
    text.push_str(&unsupported_rejection_rows());
    text
}

fn markdown_inventory_rows() -> String {
    let mut text =
        String::from("| Id | Source | Ownership | Terlan capability | Extraction status |\n");
    text.push_str("| --- | --- | --- | --- | --- |\n");
    for entry in INVENTORY {
        text.push_str(&format!(
            "| {} | source | {} | {} | {} |\n",
            entry.id,
            entry.ownership.as_str(),
            entry.capability,
            entry.status.as_str()
        ));
    }
    text
}

fn unsupported_rejection_rows() -> String {
    let mut text = String::from("| Id | Diagnostic |\n");
    text.push_str("| --- | --- |\n");
    for entry in INVENTORY {
        let Some(diagnostic) = entry.unsupported_diagnostic else {
            continue;
        };
        text.push_str(&format!("| {} | `{}` |\n", entry.id, diagnostic));
    }
    text
}
