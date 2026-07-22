use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn native_no_std_target_feasibility_writes_complete_report() {
    let root = TempRepo::new("native_no_std_report");
    let summary = run_native_no_std_target_feasibility(root.path()).expect("quality gate");

    assert_eq!(7, summary.target_count);
    assert_eq!(12, summary.feature_count);
    assert!(summary.rejected_feature_count >= 8);
    assert_eq!(8, summary.adversarial_case_count);

    let report = fs::read_to_string(&summary.report_path).expect("report");
    assert!(report.contains("terlan.native-no-std-target-feasibility.v1"));
    assert!(report.contains("feasibility-contract-only"));
    assert!(report.contains("bare-metal-no-std"));
    assert!(report.contains("risc-v-soc"));
    assert!(report.contains("native_target_unsupported_feature"));
    assert!(!report.contains("/home/"));
    assert!(!report.contains("/tmp/"));
}

#[test]
fn native_no_std_target_feasibility_accepts_only_pure_minimal_fixture() {
    let targets = target_matrix();
    let bare = target(&targets, "bare-metal-no-std").expect("target");

    let diagnostics = validate_source_fixture(
        bare,
        "import std.core.Int.\npub add(a: Int, b: Int): Int -> a + b.",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn native_no_std_target_feasibility_rejects_ambient_os_and_runtime_features() {
    let targets = target_matrix();
    let bare = target(&targets, "bare-metal-no-std").expect("target");
    let source = [
        "import std.io.File.",
        "import std.net.Tcp.",
        "import std.process.Command.",
        "import std.vm.Actor.",
        "import std.vm.Blocking.",
        "import std.native.Boundary.",
        "import std.collections.Map.",
    ]
    .join("\n");

    let diagnostics = validate_source_fixture(bare, &source);

    assert_eq!(7, diagnostics.len());
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code == DIAGNOSTIC_CODE));
    assert_eq!(1, diagnostics[0].line);
    assert_eq!(7, diagnostics[6].line);
    assert_eq!(Some("os.filesystem"), diagnostics[0].required_capability);
    assert_eq!(Some("target.heap"), diagnostics[6].required_capability);
}

#[test]
fn native_no_std_target_feasibility_rejects_duplicate_targets() {
    let mut targets = target_matrix();
    targets.push(targets[0].clone());

    let error = validate_target_matrix(&targets).expect_err("duplicate must fail");

    assert!(error.contains("duplicate target row `native-host`"));
}

#[test]
fn native_no_std_target_feasibility_rejects_ambient_runtime_permission() {
    let mut targets = target_matrix();
    set_support(
        &mut targets[0].features,
        "ambient-runtime",
        SupportClass::HostOsRequired,
        None,
    );

    let error = validate_target_matrix(&targets).expect_err("ambient runtime must fail");

    assert!(error.contains("permits an ambient runtime"));
}

#[test]
fn native_no_std_target_feasibility_rejects_actor_on_vm_free_target() {
    let mut targets = target_matrix();
    let bare = targets
        .iter_mut()
        .find(|row| row.target == "bare-metal-no-std")
        .expect("target");
    set_support(
        &mut bare.features,
        "actors",
        SupportClass::ReducedVm,
        Some("runtime.vm"),
    );

    let error = validate_target_matrix(&targets).expect_err("actor use must fail");

    assert!(error.contains("VM-free target `bare-metal-no-std` permits actor execution"));
}

#[test]
fn native_no_std_target_feasibility_report_is_deterministic() {
    let first = TempRepo::new("native_no_std_first");
    let second = TempRepo::new("native_no_std_second");
    let first_summary = run_native_no_std_target_feasibility(first.path()).expect("first");
    let second_summary = run_native_no_std_target_feasibility(second.path()).expect("second");

    let first_report = fs::read(first_summary.report_path).expect("first report");
    let second_report = fs::read(second_summary.report_path).expect("second report");

    assert_eq!(first_report, second_report);
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
