use super::{run_otp_runtime_exit, validate_otp_runtime_exit_text};

#[test]
fn otp_runtime_exit_doc_accepts_current_inventory() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let summary =
        run_otp_runtime_exit(&root).expect("current OTP runtime exit inventory should be valid");
    assert_eq!(summary.removal_lane_count, 6);
    assert_eq!(summary.closeout_blocker_count, 0);
}

#[test]
fn otp_runtime_exit_text_rejects_missing_removal_lane() {
    let text = "0.0.7 exit condition no active stock OTP runtime dependency terlan-vm std.vm migration bridge reference-only not compatibility gates remove the generated Erlang default path remove the `erlc` execution path remove the `erl` runtime invocation";
    let diagnostics = validate_otp_runtime_exit_text(text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing OTP removal lane")),
        "expected missing removal lane diagnostic: {diagnostics:?}"
    );
}

#[test]
fn otp_runtime_exit_text_rejects_compatibility_contract_claim() {
    let mut text = include_str!("../../../../docs/runtime/OTP_RUNTIME_EXIT.md").to_string();
    text.push_str("\nOTP is the 0.0.7 runtime contract.\n");
    let diagnostics = validate_otp_runtime_exit_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("forbidden OTP runtime claim")),
        "expected forbidden claim diagnostic: {diagnostics:?}"
    );
}

#[test]
fn otp_runtime_exit_text_rejects_stale_beam_serve_lane() {
    let mut text = include_str!("../../../../docs/runtime/OTP_RUNTIME_EXIT.md").to_string();
    text.push_str("\n`terlc serve` BEAM-backed handler lane\n");
    let diagnostics = validate_otp_runtime_exit_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("forbidden OTP runtime claim")),
        "expected forbidden stale serve lane diagnostic: {diagnostics:?}"
    );
}

#[test]
fn otp_runtime_exit_text_rejects_placeholder_wording() {
    let mut text = include_str!("../../../../docs/runtime/OTP_RUNTIME_EXIT.md").to_string();
    text.push_str("\nTODO: decide whether OTP runtime is still required.\n");
    let diagnostics = validate_otp_runtime_exit_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder OTP runtime exit text")),
        "expected placeholder diagnostic: {diagnostics:?}"
    );
}
