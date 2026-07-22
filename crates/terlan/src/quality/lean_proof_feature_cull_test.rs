use super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan_lean_feature_cull_{}_{}",
            std::process::id(),
            nonce
        ));
        fs::create_dir_all(&root).expect("create fixture root");
        Self { root }
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
        fs::write(path, text).expect("write fixture");
    }

    fn complete() -> Self {
        let fixture = Self::new();
        fixture.write("Makefile", &replacement_make_targets());
        fixture.write("crates/terlan/cli.mk", "");
        fixture.write(PROOF_PATH, &proof_source());
        fixture.write(ARTIFACT_PATH, &artifact_metadata());
        fixture.write(
            INVENTORY_PATH,
            &format!("path\tstatus\n{PROOF_PATH}\tcurrent\tformal boundary\n"),
        );
        fixture.write(
            ARTIFACTS_PATH,
            &format!("path\tstatus\tscope\tmetadata\n{PROOF_PATH}\tcurrent\trejection\t{ARTIFACT_PATH}\n"),
        );
        for path in ACTIVE_MANIFESTS {
            if *path != ARTIFACTS_PATH {
                fixture.write(path, "current\n");
            }
        }
        fixture
    }

    fn map(&self) -> FeatureCullMap {
        serde_json::from_str(&map_json()).expect("parse fixture map")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const PROOF_PATH: &str = "proofs/lean/Terlan/FeatureCull/LegacyBoundaries.lean";
const ARTIFACT_PATH: &str = "proofs/lean/artifacts/feature-cull-legacy-boundaries.json";

#[test]
fn feature_cull_accepts_complete_formal_boundary() {
    let fixture = Fixture::complete();
    let diagnostics = validate_feature_cull(&fixture.root, &fixture.map());
    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
}

#[test]
fn feature_cull_rejects_missing_required_feature() {
    let fixture = Fixture::complete();
    let mut map = fixture.map();
    map.features.remove(0);
    let diagnostics = validate_feature_cull(&fixture.root, &map);
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("complete and sorted")));
}

#[test]
fn feature_cull_rejects_stale_active_manifest_term() {
    let fixture = Fixture::complete();
    fixture.write(
        "docs/compiler/proof_track/lean_proof_gaps.tsv",
        "fallback uses core-v0\n",
    );
    let diagnostics = validate_feature_cull(&fixture.root, &fixture.map());
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("core_v0_profile")));
}

#[test]
fn feature_cull_rejects_restored_make_alias() {
    let fixture = Fixture::complete();
    fixture.write("crates/terlan/cli.mk", "vm-profile-check:\n\t@true\n");
    let diagnostics = validate_feature_cull(&fixture.root, &fixture.map());
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("has been restored")));
}

#[test]
fn feature_cull_rejects_missing_rejection_theorem() {
    let fixture = Fixture::complete();
    fixture.write(
        PROOF_PATH,
        "theorem noProofArtifactUsesRetiredAssumptions : True := by trivial\n",
    );
    let diagnostics = validate_feature_cull(&fixture.root, &fixture.map());
    assert!(diagnostics.iter().any(|item| item.contains("missing from")));
}

#[test]
fn feature_cull_rejects_unsorted_forbidden_terms() {
    let fixture = Fixture::complete();
    let mut map = fixture.map();
    map.features[0].forbidden_terms.reverse();
    let diagnostics = validate_feature_cull(&fixture.root, &map);
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("forbidden terms")));
}

fn map_json() -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../proofs/lean/feature_cull/removed_features.json"),
    )
    .expect("read repository feature-cull map")
}

fn replacement_make_targets() -> String {
    REQUIRED_FEATURES
        .iter()
        .zip([
            "runtime-aot-only-check",
            "target-inference-default-vm-check",
            "target-inference-contract-check",
            "all-terlan-tests-vm-check",
            "function-head-pattern-parameters-check",
            "native-boundary-terminology-check",
            "target-inference-default-vm-check",
        ])
        .map(|(_, target)| format!("{target}:\n\t@true\n"))
        .collect()
}

fn proof_source() -> String {
    let map: FeatureCullMap = serde_json::from_str(&map_json()).expect("parse map");
    let mut proof = map
        .features
        .iter()
        .map(|feature| {
            let name = feature.theorem.rsplit('.').next().expect("theorem name");
            format!("theorem {name} : True := by trivial\n")
        })
        .collect::<String>();
    proof.push_str("theorem everyRetiredAssumptionIsBlockedBeforeVm : True := by trivial\n");
    proof.push_str("theorem noProofArtifactUsesRetiredAssumptions : True := by trivial\n");
    proof
}

fn artifact_metadata() -> String {
    let map: FeatureCullMap = serde_json::from_str(&map_json()).expect("parse map");
    serde_json::json!({
        "theorem_names": map.features.iter().map(|feature| &feature.theorem).collect::<Vec<_>>()
    })
    .to_string()
}
