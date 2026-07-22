use super::*;

/// Verifies summary category counts stay coherent.
///
/// Inputs:
/// - Static VM ownership inventory.
///
/// Output:
/// - Test passes when ownership category counts add up to the inventory size.
///
/// Transformation:
/// - Locks the success summary to the inventory table so command output stays
///   internally consistent as entries are changed.
#[test]
fn summary_counts_cover_all_inventory_entries() {
    let summary = summary();
    assert_eq!(
        summary.inventory_count,
        summary.compiler_owned_count
            + summary.vm_owned_count
            + summary.boundary_owned_count
            + summary.reference_only_count
            + summary.out_of_contract_count
    );
}

/// Verifies the contract text validator accepts the required ownership terms.
///
/// Inputs:
/// - A compact contract text containing every required term.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Exercises the semantic term gate without reading repository files.
#[test]
fn contract_text_accepts_required_terms() {
    let text = REQUIRED_TERMS.join("\n");
    assert!(validate_contract_text(&text).is_empty());
}

/// Verifies forbidden compatibility claims are rejected.
///
/// Inputs:
/// - A contract text containing required terms plus a forbidden claim.
///
/// Output:
/// - Test passes when the forbidden claim is reported.
///
/// Transformation:
/// - Prevents the ownership contract from drifting back into OTP/VM
///   compatibility as the default runtime premise.
#[test]
fn contract_text_rejects_forbidden_claims() {
    let text = format!(
        "{}\nOTP compatibility is the runtime contract.",
        REQUIRED_TERMS.join("\n")
    );
    let diagnostics = validate_contract_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("otp compatibility")),
        "diagnostics should reject OTP runtime-contract claim: {diagnostics:?}"
    );
}

/// Verifies consensus algorithms cannot drift into the VM ownership contract.
///
/// Inputs:
/// - A contract text containing required terms plus a Paxos ownership claim.
///
/// Output:
/// - Test passes when the forbidden Paxos claim is reported.
///
/// Transformation:
/// - Locks the runtime boundary: the VM owns reliable distributed primitives,
///   not consensus algorithms such as Paxos or VSR.
#[test]
fn contract_text_rejects_vm_owned_paxos_claims() {
    let text = format!("{}\nPaxos is VM-owned.", REQUIRED_TERMS.join("\n"));
    let diagnostics = validate_contract_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("paxos")),
        "diagnostics should reject Paxos VM ownership claim: {diagnostics:?}"
    );
}

/// Verifies VSR is also rejected as a VM-owned consensus algorithm.
///
/// Inputs:
/// - A contract text containing required terms plus a VSR ownership claim.
///
/// Output:
/// - Test passes when the forbidden VSR claim is reported.
///
/// Transformation:
/// - Prevents persistent-actor planning from silently turning an algorithm
///   choice into a default VM runtime contract.
#[test]
fn contract_text_rejects_vm_owned_vsr_claims() {
    let text = format!("{}\nThe VM provides VSR.", REQUIRED_TERMS.join("\n"));
    let diagnostics = validate_contract_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("vsr")),
        "diagnostics should reject VSR VM ownership claim: {diagnostics:?}"
    );
}
