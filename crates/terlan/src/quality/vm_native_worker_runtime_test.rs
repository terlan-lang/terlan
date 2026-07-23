use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_native_worker_runtime, REQUIRED_CANONICAL_RUST_TESTS};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-native-worker-runtime-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/src/commands/emit_native_metadata/artifacts.rs",
            r#"
#![forbid(unsafe_code)]
NativeBoundaryWorker NativeBoundaryCommand NativeBoundaryReply NativeBoundaryValue NativeBoundaryHandle
Register { request_id Call { request_id Dispose { request_id Stop
DEFAULT_CREDIT_WINDOW credit_window request_stop send_and_recv validate_args
validate_handle stale_native_handle native_worker_stopped
native_operation_unimplemented native_operation_unknown
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
block wake request_cancellation charge_reductions cancellation_requested
resource_handles exit
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
VmResourceTable VmResourceTransferPolicy register get_for_owner transfer release
cleanup_owner stale native resource handle
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/scheduler.rs",
            r#"
VmScheduler wake_process request_cancellation charge_reductions VmSchedulerDecision
VmSchedulerOutcome Cancelled reductions_charged
"#,
        )?;
        self.write(
            "docs/runtime/NATIVE_BOUNDARY_GLOSSARY.md",
            r#"
capability validation resource ownership scheduler accounting cancellation
backpressure typed failure propagation VM-owned semantics
"#,
        )?;
        for (relative, tests) in REQUIRED_CANONICAL_RUST_TESTS {
            let source = tests
                .iter()
                .map(|test| format!("fn {test}() {{}}"))
                .collect::<Vec<_>>()
                .join("\n");
            self.write(relative, &source)?;
        }
        self.write(
            "Makefile",
            r#"
COMPLETED_SLICE_RUST_GATES := \
	vm-native-worker-runtime-check
$(COMPLETED_SLICE_RUST_GATES): $(CANONICAL_RUST_SUITE_OWNER)

vm-native-worker-runtime-check: terlan-vm-artifact-format-check stdlib-native-artifacts-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-native-worker-runtime
"#,
        )
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn vm_native_worker_runtime_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_native_worker_runtime(repo.root()).expect("quality check");

    assert_eq!(summary.policy_count, 6);
    assert_eq!(summary.trace_case_count, 8);
    assert_eq!(summary.rejected_runtime_count, 8);
    assert_eq!(summary.canonical_rust_test_count, 15);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-native-worker-runtime-report-v2"));
    assert!(report.contains("workerPolicyMatrix"));
    assert!(report.contains("actorParkResumeTraces"));
    assert!(report.contains("requestLifecycleAdversarialSelectors"));
    assert!(report.contains("staleResultRejection"));
    assert!(report.contains("scheduler-integrated native dispatch"));
}

#[test]
fn vm_native_worker_runtime_rejects_missing_worker_anchor() {
    let repo = TestRepo::new("missing-worker-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let artifacts = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/commands/emit_native_metadata/artifacts.rs"),
    )
    .expect("read artifacts");
    repo.write(
        "crates/terlan/src/commands/emit_native_metadata/artifacts.rs",
        &artifacts.replace("stale_native_handle", ""),
    )
    .expect("rewrite artifacts");

    let error = run_vm_native_worker_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("stale_native_handle"));
}

#[test]
fn vm_native_worker_runtime_rejects_missing_runtime_anchor() {
    let repo = TestRepo::new("missing-runtime-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let scheduler = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/scheduler.rs"),
    )
    .expect("read scheduler");
    repo.write(
        "crates/terlan/src/runtime/vm/scheduler.rs",
        &scheduler.replace("reductions_charged", ""),
    )
    .expect("rewrite scheduler");

    let error = run_vm_native_worker_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("reductions_charged"));
}

#[test]
fn vm_native_worker_runtime_rejects_missing_canonical_test() {
    let repo = TestRepo::new("missing-canonical-test").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let relative = "crates/terlan/src/commands/emit_native_metadata/artifacts_test.rs";
    let source = fs::read_to_string(repo.root().join(relative)).expect("read canonical tests");
    repo.write(
        relative,
        &source.replace(
            "fn native_boundary_rust_stub_compiles_as_library()",
            "fn renamed_worker_contract_test()",
        ),
    )
    .expect("rewrite makefile");

    let error = run_vm_native_worker_runtime(repo.root()).expect_err("canonical test should fail");

    assert!(error.contains("native_boundary_rust_stub_compiles_as_library"));
}
