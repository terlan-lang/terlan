use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::run_vm_http_benchmark_comparability;

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
            "terlan-vm-http-benchmark-contract-{name}-{}-{unique}",
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
        self.write("benches/http/PROFILE.toml", COMPLETE_PROFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_PROFILE: &str = r#"
schema = "terlan-vm-http-comparability-profile-v1"
sample_count = 3
regression_threshold_percent = 15
stacks = ["terlan-vm", "axum", "hyper"]
metrics = ["mean_us", "p50_us", "p95_us", "p99_us", "throughput_requests_per_second"]

[schedule]
fixed_total_requests = 3000
warmup_requests = 300
protocol = "http1"
tls_mode = "disabled-for-all-stacks"
parser_mode = "full-stack-parser"
keep_alive_policy = "matched-per-lane"
concurrency = [1, 10, 100, 1000]
payload_bytes = [0, 512, 4096]
route_mix = ["static", "json", "add", "route-param", "stateful-counter"]

[replay]
fingerprint_schema = "terlan.vm.multicore-replay.v1"
execution_validation_required = true
stable_runs_required = 3

[adversarial]
scenarios = ["malformed-headers", "large-headers", "slow-client", "cancellation", "backpressure"]
"#;

#[test]
fn comparability_contract_writes_fingerprinted_report() {
    let repo = TestRepo::new("comparability").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_benchmark_comparability(repo.root()).expect("quality gate");

    assert_eq!(summary.profile_fingerprint.len(), 64);
    assert_eq!(summary.concurrency_count, 4);
    assert_eq!(summary.scenario_count, 5);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-benchmark-comparability-contract-v1"));
    assert!(report.contains("stateful-counter"));
}

#[test]
fn comparability_contract_rejects_too_few_samples() {
    let repo = TestRepo::new("samples").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "benches/http/PROFILE.toml",
        &COMPLETE_PROFILE.replace("sample_count = 3", "sample_count = 2"),
    )
    .expect("rewrite profile");

    let error = run_vm_http_benchmark_comparability(repo.root()).expect_err("must fail");
    assert!(error.contains("at least three stable runs"));
}

#[test]
fn comparability_contract_rejects_missing_adversarial_scenario() {
    let repo = TestRepo::new("scenario").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "benches/http/PROFILE.toml",
        &COMPLETE_PROFILE.replace(", \"backpressure\"", ""),
    )
    .expect("rewrite profile");

    let error = run_vm_http_benchmark_comparability(repo.root()).expect_err("must fail");
    assert!(error.contains("adversarial scenario `backpressure`"));
}
