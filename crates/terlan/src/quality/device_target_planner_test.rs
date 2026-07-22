use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn device_target_planner_writes_report_for_builtin_profiles() {
    let root = TempRepo::new("device_target_planner_writes_report");

    let summary = run_device_target_planner(root.path()).expect("device target planner");

    assert_eq!(2, summary.profile_count);
    assert_eq!(2, summary.plan_hash_count);
    assert_eq!(8, summary.rejected_feature_count);
    assert!(summary.diagnostic_count >= 8);
    assert_eq!(6, summary.future_lowering_prerequisite_count);

    let report = fs::read_to_string(
        root.path()
            .join("target/quality/device-target-planner-report.json"),
    )
    .expect("read report");
    assert!(report.contains("terlan.device-target-planner.v1"));
    assert!(report.contains("nxt-arm7-constrained"));
    assert!(report.contains("riscv32imac-generic"));
    assert!(report.contains("unsupported http"));
    assert!(report.contains("VM-free no_std allocator integration"));
}

#[test]
fn device_target_planner_rejects_missing_profile_fields() {
    let error = parse_device_profile(
        r#"{
          "name": "bad-profile",
          "cpu": "rv32"
        }"#,
    )
    .expect_err("missing fields should fail");

    assert!(error.contains("missing device profile field `memory_budget_bytes`"));
    assert!(error.contains("missing device profile field `rust_target`"));
}

#[test]
fn device_target_planner_rejects_inconsistent_memory_budget() {
    let error = parse_device_profile(
        r#"{
          "name": "tiny",
          "cpu": "rv32",
          "memory_budget_bytes": 1024,
          "allocator_policy": "static-region",
          "panic_strategy": "abort",
          "runtime_profile": "no_std.static",
          "peripherals": [],
          "package_hal_capabilities": [],
          "linker_output_format": "elf",
          "rust_target": "riscv32imac-unknown-none-elf",
          "unsupported_terlan_features": [],
          "producible_artifacts": ["device-plan.json"]
        }"#,
    )
    .expect_err("tiny memory budget should fail");

    assert!(error.contains("minimum supported planning budget"));
}

#[test]
fn device_target_planner_rejects_undeclared_hal_capability() {
    let error = parse_device_profile(
        r#"{
          "name": "missing-hal",
          "cpu": "rv32",
          "memory_budget_bytes": 8192,
          "allocator_policy": "static-region",
          "panic_strategy": "abort",
          "runtime_profile": "no_std.static",
          "peripherals": ["uart"],
          "package_hal_capabilities": [],
          "linker_output_format": "elf",
          "rust_target": "riscv32imac-unknown-none-elf",
          "unsupported_terlan_features": [],
          "producible_artifacts": ["device-plan.json"]
        }"#,
    )
    .expect_err("missing HAL capability should fail");

    assert!(error.contains("package/HAL mismatch"));
    assert!(error.contains("hal.uart"));
}

#[test]
fn device_target_planner_rejects_unproducible_artifact_claims() {
    let error = parse_device_profile(
        r#"{
          "name": "bad-artifact",
          "cpu": "rv32",
          "memory_budget_bytes": 8192,
          "allocator_policy": "static-region",
          "panic_strategy": "abort",
          "runtime_profile": "no_std.static",
          "peripherals": [],
          "package_hal_capabilities": [],
          "linker_output_format": "elf",
          "rust_target": "riscv32imac-unknown-none-elf",
          "unsupported_terlan_features": [],
          "producible_artifacts": ["firmware.bin"]
        }"#,
    )
    .expect_err("unproducible artifact should fail");

    assert!(error.contains("claims artifacts the compiler cannot produce"));
}

#[test]
fn device_target_planner_orders_plans_deterministically_and_rejects_imports() {
    let profile = parse_device_profile(nxt_profile_json()).expect("profile");
    let project = BTreeMap::from([
        (
            "b.terl".to_string(),
            "module B.\nimport std.http.Server.\nimport std.core.Int.".to_string(),
        ),
        (
            "a.terl".to_string(),
            "module A.\nimport std.db.Postgres.\nimport std.io.File.".to_string(),
        ),
    ]);

    let first = plan_device_target(&project, &profile).expect("first plan");
    let second = plan_device_target(&project, &profile).expect("second plan");

    assert_eq!(first.plan_hash, second.plan_hash);
    assert_eq!(first.rejected_imports, second.rejected_imports);
    assert!(first
        .rejected_imports
        .iter()
        .any(|entry| entry.contains("std.http.Server rejected")));
    assert!(first
        .rejected_imports
        .iter()
        .any(|entry| entry.contains("std.io.File rejected")));
}

#[test]
fn device_target_planner_rejects_source_checkout_path_leakage() {
    let plan = DeviceTargetPlan {
        profile_name: "leaky".to_string(),
        selected_runtime: "no_std.static".to_string(),
        std_subset: vec![],
        package_capabilities: vec![],
        native_bindings: vec![],
        memory_policy: "static-region:abort bytes:8192".to_string(),
        rejected_imports: vec!["/tmp/source/std.http.Server".to_string()],
        required_toolchains: vec![],
        output_artifacts: vec![],
        diagnostics: vec![],
        plan_hash: "0".to_string(),
    };

    let error = validate_plan_no_path_leakage(&plan).expect_err("path leakage should fail");

    assert!(error.contains("leaked a source checkout path"));
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        fs::create_dir_all(&path).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
