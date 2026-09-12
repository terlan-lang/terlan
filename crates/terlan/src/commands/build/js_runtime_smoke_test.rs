//! Real Node integration while preserving the compiler's optional-runtime contract.

use super::*;

#[test]
fn node_syntax_smoke_accepts_valid_and_rejects_invalid_javascript() {
    let root = crate::support::test_fs::temp_dir("js_runtime_smoke", "node_syntax");
    let valid = root.join("valid.js");
    let invalid = root.join("invalid.js");
    fs::write(&valid, "const answer = 42;\n").unwrap();
    fs::write(&invalid, "const = ;\n").unwrap();
    let outcome = run_js_runtime_smoke(&valid).unwrap();
    match outcome.as_str() {
        "passed" => {
            let error = run_js_runtime_smoke(&invalid).unwrap_err();
            assert!(error.contains("error[js_validate_runtime]"), "{error}");
            println!("Node executed: valid syntax accepted; invalid syntax rejected");
        }
        "skipped:node_unavailable" => {
            assert_eq!(run_js_runtime_smoke(&invalid).unwrap(), outcome);
            println!("Node absent: optional-runtime contract verified without Node execution");
        }
        other => panic!("unexpected runtime smoke outcome: {other}"),
    }
    fs::remove_dir_all(root).unwrap();
}
