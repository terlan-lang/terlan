use std::fs;

use crate::terlan_quality::support::make_quality_temp_dir;

use super::*;

fn write_contract_fixture(root: &Path) {
    for required in REQUIRED_FILES {
        let path = root.join(required.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create ABI fixture parent");
        }
        fs::write(path, required.markers.join("\n")).expect("write ABI fixture owner");
    }
    for (path, _) in FORBIDDEN_SOURCE_FRAGMENTS {
        let target = root.join(path);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("create forbidden fixture parent");
        }
        fs::write(target, "canonical NativeBoundary implementation\n")
            .expect("write forbidden fixture owner");
    }
}

#[test]
fn abi1_pre_freeze_accepts_complete_current_contract() {
    let root = make_quality_temp_dir("abi1_pre_freeze_complete");
    write_contract_fixture(&root);
    let summary = run_abi1_pre_freeze(&root).expect("complete ABI contract");
    assert_eq!(summary.file_count, REQUIRED_FILES.len());
    assert!(summary.required_marker_count > 20);
    assert!(summary.forbidden_fragment_count > 0);
    fs::remove_dir_all(root).expect("remove ABI fixture");
}

#[test]
fn abi1_pre_freeze_rejects_missing_admission_marker() {
    let root = make_quality_temp_dir("abi1_pre_freeze_missing_marker");
    write_contract_fixture(&root);
    let path = root.join("crates/terlan/src/runtime/native_image/descriptor.rs");
    fs::write(&path, "validate_descriptor(descriptor)?\n").expect("replace descriptor fixture");
    let error = run_abi1_pre_freeze(&root).expect_err("missing marker must fail");
    assert!(error.contains("validate_managed_layouts"));
    fs::remove_dir_all(root).expect("remove ABI fixture");
}

#[test]
fn abi1_pre_freeze_rejects_trusted_in_shard_shortcut() {
    let root = make_quality_temp_dir("abi1_pre_freeze_shortcut");
    write_contract_fixture(&root);
    let path = root.join("crates/terlan/src/runtime/native_boundary/capability_sandbox.rs");
    fs::write(&path, "trusted_in_shard = true\n").expect("write unsafe shortcut");
    let error = run_abi1_pre_freeze(&root).expect_err("shortcut must fail");
    assert!(error.contains("forbidden ABI 1 shortcut `trusted_in_shard`"));
    fs::remove_dir_all(root).expect("remove ABI fixture");
}
