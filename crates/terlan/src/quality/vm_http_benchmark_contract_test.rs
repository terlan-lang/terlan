use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_http_benchmark_comparability, run_vm_http_runtime_attribution_contract};

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
        self.write("benches/http/PROFILE.toml", COMPLETE_PROFILE)?;
        self.write(
            "crates/terlan/src/vm/main/http_attribution.rs",
            "transportNs parserNs schedulerNs routingNs allocationAndConversionNs handlerNs responseWriteNs completedMatchesReductions phaseBucketsMatchAccountedTotal queueBalanced parkedProcessesReleased saturationHasBackpressureOutcome",
        )?;
        self.write(
            "crates/terlan/src/vm/main/http_benchmark_handlers.rs",
            "terlan-vm-http-replay-v1 fingerprintSha256 executionValidated",
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
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
fingerprint_schema = "terlan-vm-http-replay-v1"
execution_validation_required = true
stable_runs_required = 3

[adversarial]
scenarios = ["malformed-headers", "large-headers", "slow-client", "cancellation", "backpressure"]
"#;

const COMPLETE_MAKEFILE: &str = r#"
CHECK_GATES := \
	vm-http-runtime-attribution-check \

VM_HTTP_BENCHMARK_COMPARABILITY_DEPS := vm-http-concurrency-investigation-check
vm-http-benchmark-comparability-check: $(VM_HTTP_BENCHMARK_COMPARABILITY_DEPS)
	cargo run -- vm-http-benchmark-comparability

vm-http-runtime-attribution-check: vm-http-benchmark-comparability-check
	cargo run -- vm-http-runtime-attribution

release-0-0-7-preflight: vm-http-runtime-attribution-check release-version-channel-check
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

#[test]
fn attribution_contract_writes_product_ownership_report() {
    let repo = TestRepo::new("attribution").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_runtime_attribution_contract(repo.root()).expect("quality gate");

    assert_eq!(summary.bucket_count, 7);
    assert_eq!(summary.invariant_count, 5);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("workspace-benchmarks-outside-golden-release"));
    assert!(report.contains("vm-http-benchmark-comparability-check"));
}

#[test]
fn attribution_contract_rejects_missing_bucket() {
    let repo = TestRepo::new("bucket").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "crates/terlan/src/vm/main/http_attribution.rs",
        "transportNs parserNs schedulerNs routingNs allocationAndConversionNs handlerNs completedMatchesReductions phaseBucketsMatchAccountedTotal queueBalanced parkedProcessesReleased saturationHasBackpressureOutcome",
    )
    .expect("rewrite attribution");

    let error = run_vm_http_runtime_attribution_contract(repo.root()).expect_err("must fail");
    assert!(error.contains("attribution bucket `responseWriteNs`"));
}

#[test]
fn attribution_contract_rejects_release_order_drift() {
    let repo = TestRepo::new("release-order").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace(
            "release-0-0-7-preflight: vm-http-runtime-attribution-check ",
            "release-0-0-7-preflight: ",
        ),
    )
    .expect("rewrite Makefile");

    let error = run_vm_http_runtime_attribution_contract(repo.root()).expect_err("must fail");
    assert!(error.contains("release-0-0-7-preflight"));
}
