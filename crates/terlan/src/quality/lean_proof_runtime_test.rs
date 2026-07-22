use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the Lean proof runtime profile accepts a complete configuration.
///
/// Inputs:
/// - Temporary `lean-proof-runner.toml` with all required groups and budgets.
///
/// Output:
/// - Summary with four groups and a generated resource-accounting report.
///
/// Transformation:
/// - Keeps proof runtime policy executable even when no Lean tree is present.
#[test]
fn lean_proof_runtime_accepts_complete_runner_config() {
    let root = temp_repo("lean_proof_runtime_accepts");
    write_runner_config(&root, complete_config());

    let summary = run_lean_proof_runtime(&root).expect("complete config should pass");

    assert_eq!(summary.group_count, 4);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("\"shared_lean_path_allowed\": false"));
    assert!(report.contains("\"name\": \"foundational\""));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies all stable scheduling groups are required.
///
/// Inputs:
/// - Runner config missing the `std-boundary` group.
///
/// Output:
/// - Diagnostic naming the missing scheduling group.
///
/// Transformation:
/// - Prevents CI from silently dropping a proof lane.
#[test]
fn lean_proof_runtime_rejects_missing_group() {
    let root = temp_repo("lean_proof_runtime_missing_group");
    write_runner_config(
        &root,
        complete_config().replace("name = \"std-boundary\"", "name = \"std_boundary\""),
    );

    let error = run_lean_proof_runtime(&root).expect_err("missing group should fail");

    assert!(error.contains("missing scheduling group `std-boundary`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies environment isolation rejects shared LEAN_PATH.
///
/// Inputs:
/// - Runner config without `LEAN_PATH` in `forbidden_env`.
///
/// Output:
/// - Diagnostic naming the missing forbidden environment variable.
///
/// Transformation:
/// - Prevents proof jobs from depending on user or CI global Lean state.
#[test]
fn lean_proof_runtime_rejects_shared_lean_path() {
    let root = temp_repo("lean_proof_runtime_shared_lean_path");
    write_runner_config(
        &root,
        complete_config().replace("\"LEAN_PATH\"", "\"PATH\""),
    );

    let error = run_lean_proof_runtime(&root).expect_err("shared LEAN_PATH should fail");

    assert!(error.contains("runner.forbidden_env must include LEAN_PATH"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies malformed budgets are rejected.
///
/// Inputs:
/// - Runner config with zero timeout for one group.
///
/// Output:
/// - Diagnostic naming positive budget requirements.
///
/// Transformation:
/// - Prevents impossible runtime envelopes from reaching CI.
#[test]
fn lean_proof_runtime_rejects_zero_budget() {
    let root = temp_repo("lean_proof_runtime_zero_budget");
    write_runner_config(
        &root,
        complete_config().replace("timeout_ms = 60000", "timeout_ms = 0"),
    );

    let error = run_lean_proof_runtime(&root).expect_err("zero budget should fail");

    assert!(error.contains("group `foundational` budgets must be positive"));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn lean_proof_runtime_rejects_unpinned_toolchain_contract() {
    let root = temp_repo("lean_proof_runtime_unpinned_toolchain");
    write_runner_config(
        &root,
        complete_config().replace("lean_version = \"4.31.0\"", "lean_version = \"latest\""),
    );

    let error = run_lean_proof_runtime(&root).expect_err("unpinned toolchain should fail");

    assert!(error.contains("runner.lean_version must be `4.31.0`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(root.join("proofs/lean/ci")).expect("create proof ci dir");
    root
}

fn write_runner_config(root: &Path, text: String) {
    fs::write(root.join(RUNNER_CONFIG_PATH), text).expect("write runner config");
}

fn complete_config() -> String {
    r#"[runner]
lockstep_mode = true
temp_root = "build/tmp/lean-proof"
clean_env = true
forbidden_env = ["LEAN_PATH"]
lean_version = "4.31.0"
elan_channel = "leanprover/lean4:v4.31.0"
lake_flags = ["env", "lean"]
dependency_lockfile = "proofs/lean/lake-manifest.json"

[guardrails]
warning_wall_time_multiplier = 2
hard_wall_time_multiplier = 4
closeout_requires_lockstep = true

[[groups]]
name = "foundational"
max_parallelism = 1
timeout_ms = 60000
cpu_ms = 60000
memory_mb = 512
io_mb = 64
retry_count = 0
lockstep = true

[[groups]]
name = "lowering"
max_parallelism = 2
timeout_ms = 90000
cpu_ms = 90000
memory_mb = 768
io_mb = 96
retry_count = 1
lockstep = false

[[groups]]
name = "runtime"
max_parallelism = 2
timeout_ms = 90000
cpu_ms = 90000
memory_mb = 768
io_mb = 96
retry_count = 1
lockstep = false

[[groups]]
name = "std-boundary"
max_parallelism = 2
timeout_ms = 90000
cpu_ms = 90000
memory_mb = 768
io_mb = 96
retry_count = 1
lockstep = false
"#
    .to_string()
}
