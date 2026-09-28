//! Actual source execution proves per-case attribution and immutable image binding.

use std::collections::BTreeSet;
use std::fs;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use super::manifest_evidence::{NativeCallableEvidence, NativeEvidenceError};
use crate::runtime::native_image::debug::tvm_debug_source_sha256;
use crate::{CliCommand, CliState};

struct EvidenceFixture(std::path::PathBuf);

impl EvidenceFixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "terlan-callable-evidence-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for EvidenceFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn manifests_attribute_native_entries_to_each_passing_or_failing_case() {
    let fixture = EvidenceFixture::new();
    let path = fixture.0.join("EvidenceTest.terl");
    let output = fixture.0.join("results.json");
    let source = r#"module evidence.EvidenceTest.
pub first_api(value: Int): Int -> value + 1.
pub second_api(value: Int): Int -> value + 2.
@test
pub first_case(): Bool -> first_api(6) == 7.
@test
pub second_case(): Bool -> second_api(7) == 9.
@test
pub failing_case(): Bool -> first_api(0) == 99.
"#;
    fs::write(&path, source).unwrap();
    let status = super::run(
        CliCommand {
            verb: Some("test".to_owned()),
            args: vec![
                path.to_string_lossy().into_owned(),
                "--emit-test-result-manifest".to_owned(),
                output.to_string_lossy().into_owned(),
            ],
        },
        CliState::default(),
    );
    assert_eq!(status, ExitCode::from(1));
    let report: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(report["passed"], 2);
    assert_eq!(report["failed"], 1);
    let evidence = &report["native_callable_evidence"];
    assert_eq!(evidence["schema"], "terlan.native-callable-evidence/v1");
    assert_eq!(evidence["image_sha256"].as_str().unwrap().len(), 64);
    let declarations = evidence["declarations"].as_array().unwrap();
    let id = |name: &str| {
        let declaration = declarations
            .iter()
            .find(|entry| entry["function"] == name)
            .unwrap();
        assert_eq!(
            declaration["source_sha256"],
            tvm_debug_source_sha256(source.as_bytes())
        );
        declaration["callable_id"].as_str().unwrap().to_owned()
    };
    let first = id("first_api");
    let second = id("second_api");
    let cases = report["tests"].as_array().unwrap();
    let hits = |name: &str| -> BTreeSet<String> {
        cases.iter().find(|entry| entry["name"] == name).unwrap()["entered_native_callables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect()
    };
    assert!(hits("first_case").contains(&first));
    assert!(!hits("first_case").contains(&second));
    assert!(hits("second_case").contains(&second));
    assert!(!hits("second_case").contains(&first));
    assert!(hits("failing_case").contains(&first));
    assert!(!hits("failing_case").contains(&second));
    assert_eq!(
        cases
            .iter()
            .find(|entry| entry["name"] == "failing_case")
            .unwrap()["status"],
        "failed"
    );
}

#[test]
fn evidence_rejects_missing_or_changed_executed_image_identity() {
    use sha2::{Digest, Sha256};

    let fixture = EvidenceFixture::new();
    let path = fixture.0.join("changed.tvm");
    let original_digest: [u8; 32] = Sha256::digest(b"original executable bytes").into();
    fs::write(&path, b"replacement executable bytes").unwrap();
    assert!(matches!(
        NativeCallableEvidence::read(&path, Some(&original_digest)),
        Err(NativeEvidenceError::Identity { .. })
    ));
    assert!(matches!(
        NativeCallableEvidence::read(&path, None),
        Err(NativeEvidenceError::Identity { .. })
    ));
    let replacement_digest: [u8; 32] = Sha256::digest(b"replacement executable bytes").into();
    assert!(matches!(
        NativeCallableEvidence::read(&path, Some(&replacement_digest)),
        Err(NativeEvidenceError::Decode { .. })
    ));
}
