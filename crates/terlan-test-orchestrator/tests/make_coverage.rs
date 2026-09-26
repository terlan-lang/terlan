//! Real Cargo/Make acceptance: passed test bodies are not replayed by gate requests.
#![cfg(unix)]

use serde_json::Value;
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

#[path = "make_coverage/publication_graph.rs"]
mod publication_graph;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "make_coverage/hosted_source.rs"]
mod hosted_source;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "make_coverage/aot.rs"]
mod aot;

const RECORD: &str = r#"
pub fn record(name: &str) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true)
        .open(std::env::var_os("TERLAN_FIXTURE_RECORD").unwrap()).unwrap();
    writeln!(file, "{name}").unwrap();
}
"#;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(root: &Path, path: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn test_module(names: &[String], prefix: &str) -> String {
    let mut modules: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut output = String::new();
    for name in names {
        if let Some((module, remaining)) = name.split_once("::") {
            modules.entry(module).or_default().push(remaining.into());
        } else {
            let qualified = format!("{prefix}{name}");
            let ignored = if matches!(qualified.as_str(), "normal" | "quality::integration") {
                ""
            } else {
                "#[ignore]"
            };
            output.push_str(&format!(
                "#[test] {ignored} fn {name}() {{ crate::record({qualified:?}); }}\n"
            ));
        }
    }
    for (module, names) in modules {
        output.push_str(&format!(
            "mod {module} {{ {} }}\n",
            test_module(&names, &format!("{prefix}{module}::"))
        ));
    }
    output
}

fn fixture() -> Fixture {
    let path = std::env::temp_dir().join(format!(
        "terlan-live-make-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let fixture = Fixture(path);
    let root = &fixture.0;
    write(root, ".gitignore", "/target\n");
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=[\"terlan\",\"support\"]\nresolver=\"2\"\n",
    );
    write(
        root,
        "rust-toolchain.toml",
        "[toolchain]\nchannel=\"1.96.0\"\nprofile=\"minimal\"\n",
    );
    write(root, "terlan/Cargo.toml", "[package]\nname=\"terlan\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[features]\nquality-tools=[]\neditor-lsp=[]\nbenchmark-tools=[]\n");
    let mut names = vec!["normal".to_owned(), "quality::integration".to_owned()];
    names.extend(
        include_str!("../../../docs/quality/RUST_VALIDATION_TIERS.tsv")
            .lines()
            .filter_map(|line| line.split('\t').next())
            .filter(|name| name.contains("::"))
            .map(String::from),
    );
    write(
        root,
        "terlan/src/lib.rs",
        format!("{RECORD}{}", test_module(&names, "")),
    );
    write(
        root,
        "terlan/src/main.rs",
        "fn main() {}\n#[test] fn binary() { terlan::record(\"binary\"); }\n",
    );
    for name in ["terlc", "terlan-vm", "terlan-native-worker"] {
        write(root, &format!("terlan/src/bin/{name}.rs"), "fn main() {}\n");
    }
    write(
        root,
        "terlan/tests/integration.rs",
        "#[test] fn integration() { terlan::record(\"terlan-integration\"); }\n",
    );
    write(
        root,
        "support/Cargo.toml",
        "[package]\nname=\"support\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
    );
    write(root, "support/src/lib.rs", format!("//! ```\n//! support::record(\"doc-a\");\n//! ```\n//! ```\n//! support::record(\"doc-b\");\n//! ```\n{RECORD}\n#[test] fn native() {{ record(\"support-native\"); }}\n"));
    write(
        root,
        "support/tests/integration.rs",
        "#[test] fn integration() { support::record(\"support-integration\"); }\n",
    );
    // Include the actual production routing, not a copied skip implementation.
    let routing = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../mk/rust-coverage.mk")
        .canonicalize()
        .unwrap();
    let quality = routing.parent().unwrap().join("code-quality.mk");
    write(
        root,
        "Makefile",
        format!(
            r#"TERLAN_RUST_ORCHESTRATOR := $(TERLAN_FIXTURE_DRIVER)
include {}
include {}
# Keep the production entry-point policy in this disposable Make graph: legacy
# success switches must fail before any producer can be selected.
ifneq ($(filter 1,$(TERLAN_RUST_SUITE_ALREADY_RUN) $(TERLAN_CHECK_ALREADY_RUN)),)
$(error legacy Rust/Make skip flags are unsupported; use the live coverage owner)
endif
.PHONY: gates nested normal native ignored missing changed no-requests swallowed
gates: rust-cargo-metadata-report normal native ignored
	$(MAKE) --no-print-directory nested
normal:
	$(RUST_TEST) -p terlan --lib normal -- --exact
native:
	$(EXACT_CARGO_TEST) -p support --test integration
ignored:
	TERLAN_TEST_CAPABILITY_WORKER=$(CURDIR)/target/debug/terlan-native-worker $(RUST_TEST) -p terlan --lib capability_worker_process_transport_runs_full_cycle -- --ignored
nested:
	$(RUST_TEST) -p terlan --lib quality::
missing:
	$(RUST_TEST) -p terlan --lib missing_test
changed:
	TERLAN_TEST_SUSPICIOUS_OPTION=changed $(RUST_TEST) -p terlan --lib normal
semantic:
	TERLAN_SEMANTIC_KERNEL_ROOT=$(CURDIR) $(RUST_TEST) -p terlan --lib normal -- --exact
semantic-wrong-root:
	TERLAN_SEMANTIC_KERNEL_ROOT=/another-checkout $(RUST_TEST) -p terlan --lib normal -- --exact
proof:
	TERLAN_LEAN_PROOF_ROOT=$(CURDIR) $(RUST_TEST) -p terlan --lib normal -- --exact
proof-wrong-root:
	TERLAN_LEAN_PROOF_ROOT=/another-checkout $(RUST_TEST) -p terlan --lib normal -- --exact
no-requests:
	true
swallowed:
	-$(RUST_TEST) -p terlan --lib missing_test
"#,
            routing.display(),
            quality.display()
        ),
    );
    fixture
}

fn command(root: &Path, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        // Cargo exposes hyphenated binary-name keys to integration tests. POSIX
        // shells drop those keys; they belong to this outer test, not the fixture.
        .env_clear()
        .envs(
            std::env::vars_os()
                .filter(|(key, _)| !key.to_string_lossy().starts_with("CARGO_BIN_EXE_")),
        )
        .current_dir(root)
        .env("PWD", root)
        .env_remove("CARGO")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("TERLAN_RUST_COVERAGE_CONTEXT")
        .env("TERLAN_FIXTURE_DRIVER", root.join("target/driver"))
        .env("TERLAN_FIXTURE_RECORD", root.join("target/bodies.txt"))
        .env(
            "TERLAN_RUST_SUITE_REPORT",
            root.join("target/quality/suite.json"),
        );
    command
}

fn run(command: &mut Command) -> bool {
    ProcessControl::new(Duration::from_secs(120))
        .run(command, |_| Ok(()))
        .is_ok()
}

fn coverage(root: &Path, gate: &str) -> Value {
    let success = run(command(root, root.join("target/driver")).args([
        "--with-cargo-coverage",
        "target/quality/suite.json",
        "--",
        "make",
        "--no-print-directory",
        gate,
    ]));
    assert_eq!(success, gate == "gates", "{gate}");
    serde_json::from_slice(
        &fs::read(root.join("target/quality/suite.json.make-coverage.json")).unwrap(),
    )
    .unwrap()
}

fn bootstrap(root: &Path) {
    assert!(run(command(root, "git").args(["init", "--quiet"])));
    assert!(run(command(root, "cargo").arg("generate-lockfile")));
    assert!(run(
        command(root, "cargo").args(["build", "--locked", "-p", "terlan", "--bins"])
    ));
    assert!(run(command(
        root,
        env!("CARGO_BIN_EXE_terlan-test-orchestrator")
    )
    .args(["--install-snapshot", "target/driver"])));
}

#[test]
fn make_reuses_live_coverage_and_rejects_uncovered_or_changed_requests() {
    let fixture = fixture();
    let root = &fixture.0;
    bootstrap(root);
    assert!(run(command(root, root.join("target/driver")).args([
        "--cargo-metadata",
        "target/quality/rust-cargo-metadata.json",
        "--",
        "cargo"
    ])));
    assert!(run(&mut command(root, root.join("target/driver"))));
    let bodies = fs::read(root.join("target/bodies.txt")).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&bodies).lines().count(),
        15 + 2 * usize::from(cfg!(target_os = "linux"))
    );
    let positive = coverage(root, "gates");
    assert_eq!(positive["decision"], "gates-covered");
    assert_eq!(positive["direct_cargo_launch_count"], 0);
    let phase = positive["phases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|phase| phase["executor"] == "make-covered-gates")
        .unwrap();
    assert_eq!(phase["test_execution"]["requester_process_count"], 5);
    for gate in ["missing", "changed", "no-requests", "swallowed"] {
        assert_eq!(coverage(root, gate)["decision"], "fail");
    }
    assert_eq!(fs::read(root.join("target/bodies.txt")).unwrap(), bodies);
    assert!(!run(command(root, "make")
        .args(["--no-print-directory", "normal"])
        .env("TERLAN_RUST_SUITE_ALREADY_RUN", "1")));
    let mut plan = command(root, root.join("target/driver"));
    plan.env("MAKEFLAGS", "n").args([
        "--with-cargo-coverage",
        "missing.json",
        "--",
        "make",
        "gates",
    ]);
    assert!(run(&mut plan));
    assert!(!root.join("missing.json.make-coverage.json").exists());
    assert_eq!(fs::read(root.join("target/bodies.txt")).unwrap(), bodies);
}

#[test]
fn make_graph_deadline_is_reported_and_timeout_does_not_replay_test_bodies() {
    let fixture = fixture();
    let root = &fixture.0;
    let makefile = fs::read_to_string(root.join("Makefile")).unwrap();
    write(root, "Makefile", format!(
        "{makefile}\ndelayed: normal\n\tsleep 1\nblocked: normal\n\tsleep 10\n\ttouch target/incorrectly-completed\n"
    ));
    bootstrap(root);
    assert!(run(command(root, root.join("target/driver")).args([
        "--cargo-metadata",
        "target/quality/rust-cargo-metadata.json",
        "--",
        "cargo",
    ])));
    assert!(run(&mut command(root, root.join("target/driver"))));
    let bodies = fs::read(root.join("target/bodies.txt")).unwrap();
    for (gate, deadline, success) in [("delayed", "5", true), ("blocked", "1", false)] {
        assert_eq!(
            run(command(root, root.join("target/driver")).args([
                "--with-cargo-coverage",
                "target/quality/suite.json",
                "--graph-timeout-seconds",
                deadline,
                "--",
                "make",
                "--no-print-directory",
                gate,
            ])),
            success
        );
        let report: Value = serde_json::from_slice(
            &fs::read(root.join("target/quality/suite.json.make-coverage.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["phase_timeout_seconds"], 1800);
        let phase = report["phases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|phase| phase["executor"] == "make-covered-gates")
            .unwrap();
        assert_eq!(
            phase["test_execution"]["graph_timeout_seconds"],
            deadline.parse::<u64>().unwrap()
        );
        assert_eq!(
            phase["outcome"],
            if success { "passed" } else { "timed-out" }
        );
        assert_eq!(fs::read(root.join("target/bodies.txt")).unwrap(), bodies);
    }
    assert!(!root.join("target/incorrectly-completed").exists());
}

fn make_block<'a>(source: &'a str, first_line: &str, terminator: &str) -> &'a str {
    let start = source
        .find(first_line)
        .expect("production Make entry point");
    let end = source[start..]
        .find(terminator)
        .expect("complete Make rule")
        + start;
    &source[start..end]
}

#[test]
fn release_refresh_keeps_shared_gate_nodes_inside_one_live_make_graph() {
    let fixture = fixture();
    let root = &fixture.0;
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let check = make_block(&source, "check: rust-test-suite\n", "\n\n");
    let graph_timeout = source
        .lines()
        .find(|line| line.starts_with("TERLAN_CHECK_GRAPH_TIMEOUT_SECONDS ?="))
        .unwrap();
    let refresh = make_block(&source, "release-evidence-refresh: export", "\n\n");
    let publication = make_block(
        &source,
        ".PHONY: publish-evidence-source-prerequisites",
        "\npublish-evidence-check:",
    );
    let publication = publication_graph::prepare(root, publication);
    let multicore = make_block(
        &source,
        ".PHONY: vm-multicore-publish-prerequisites",
        "\nvm-multicore-publish-check:",
    );
    let suite = make_block(
        &source,
        "ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)",
        "\nendif",
    );
    let compose = make_block(
        &source,
        "ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)\nrelease-evidence-compose:",
        "\nendif",
    );
    let guard = make_block(
        &source,
        "ifneq ($(word 2,$(filter check release-check release-evidence-refresh,$(MAKECMDGOALS))),)",
        "\nendif",
    );
    let gates = fs::read_to_string(root.join("Makefile")).unwrap().replace(
        "normal:\n",
        "normal:\n\t@echo normal >> target/gate-entries\n",
    );
    write(
        root,
        "Makefile",
        format!(
            r#"{gates}
CARGO := cargo --locked
{graph_timeout}
VM_MULTICORE_PUBLISH_LOCAL_GATES := normal native
AOT_RELEASE_LOCAL_GATES := normal ignored
AOT_RELEASE_CARGO_CHECK := test ! -s target/publication-source-failure && $(CARGO) check -p terlan
vm-multicore-release-contract-check:
tvm-aot-release-closeout-contract-check:
terlan-release-promotion-bootstrap:
{publication}
{multicore}
TERLAN_VALIDATOR_BUILD_JOBS := 1
RELEASE_VERSION := fixture
RELEASE_EVIDENCE_GATES := normal release-only release-generated-artifacts-check rust-test-suite
.PHONY: check-gates release-only check release-evidence-refresh release-evidence-compose rust-test-suite terlan-compiler-bootstrap terlan-quality-tools-bootstrap terlan-self-validation-bootstrap terlan-release-closeout-image-bootstrap
terlan-compiler-bootstrap:
terlan-quality-tools-bootstrap:
	@echo quality >> target/bootstraps
terlan-self-validation-bootstrap:
	@echo validators >> target/bootstraps
terlan-release-closeout-image-bootstrap:
	@echo release >> target/bootstraps
check-gates: gates
release-only: terlan-release-closeout-bootstrap
	@if test "$(TERLAN_RUST_COVERAGE_SCOPE)" = hosted-source; then $(TERLAN_FIXTURE_STAGE) consume; fi
	$(RUST_TEST) -p terlan --lib quality::
{check}

{refresh}

{suite}
endif
{compose}
endif
{guard}
endif
hosted-change-source: normal
	@echo changed >> terlan/src/lib.rs
"#
        ),
    );
    write(
        root,
        "scripts/download_validated_release_artifacts.sh",
        include_bytes!("../../../scripts/download_validated_release_artifacts.sh"),
    );
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    aot::prepare(root);
    bootstrap(root);
    assert!(run(command(root, "git").args(["add", "."])));
    assert!(run(command(root, "git").args([
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "fixture",
    ])));
    let revision = ProcessControl::new(Duration::from_secs(30))
        .capture_stdout(
            command(root, "git").args(["rev-parse", "HEAD"]),
            128,
            |_| Ok(()),
        )
        .unwrap();
    let revision = std::str::from_utf8(&revision).unwrap().trim();
    assert!(!run(command(root, "make").args([
        "--no-print-directory",
        "check",
        "release-evidence-refresh"
    ])));
    assert!(!root.join("target/bodies.txt").exists());
    assert!(run(command(root, "make")
        .args(["--no-print-directory", "release-evidence-refresh"])
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_REPOSITORY", "fixture/repository")
        .env("GITHUB_SHA", revision)
        .env(
            "GITHUB_WORKFLOW_REF",
            "fixture/repository/.github/workflows/ci.yml@refs/heads/main"
        )
        .env("GITHUB_JOB", "check")
        .env("GITHUB_RUN_ID", "123456")
        .env("GITHUB_RUN_ATTEMPT", "2")));
    assert_eq!(
        fs::read_to_string(root.join("target/bodies.txt"))
            .unwrap()
            .lines()
            .count(),
        15 + 2 * usize::from(cfg!(target_os = "linux"))
    );
    assert_eq!(
        fs::read_to_string(root.join("target/gate-entries")).unwrap(),
        "normal\n"
    );
    let mut bootstraps = fs::read_to_string(root.join("target/bootstraps"))
        .unwrap()
        .lines()
        .map(String::from)
        .collect::<Vec<_>>();
    bootstraps.sort();
    assert_eq!(bootstraps, ["quality", "release", "validators"]);
    let suite: Value = serde_json::from_slice(
        &fs::read(root.join("target/quality/rust-test-suite-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(suite["decision"], "pass");
    assert_eq!(suite["direct_cargo_launch_count"], 3);
    let coverage: Value = serde_json::from_slice(
        &fs::read(root.join("target/quality/rust-test-suite-report.json.make-coverage.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(coverage["decision"], "gates-covered");
    assert_eq!(coverage["direct_cargo_launch_count"], 0);
    let phase = coverage["phases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|phase| phase["executor"] == "make-covered-gates")
        .unwrap();
    assert_eq!(phase["test_execution"]["requester_process_count"], 7);
    assert_eq!(
        phase["test_execution"]["producer_context"]["run_id"],
        123456
    );
    assert_eq!(
        phase["test_execution"]["producer_context"]["run_attempt"],
        2
    );
    assert_eq!(
        phase["test_execution"]["producer_context"]["authentication_required"],
        true
    );
    assert_eq!(
        phase["test_execution"]["make_invocation_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        phase["test_execution"]["completed_suite_binding"]["sha256"],
        sha2::Sha256::digest(
            fs::read(root.join("target/quality/rust-test-suite-report.json")).unwrap()
        )
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
    );
    assert_eq!(
        phase["test_execution"]["completed_suite_binding"]["run_id"],
        suite["run_id"]
    );
    assert_eq!(
        phase["test_execution"]["completed_suite_binding"]["selections_sha256"],
        suite["test_selection_binding"]["sha256"]
    );
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let summary = check_hosted_records(root, &coverage, revision);
        hosted_source::exercise(root, revision, &summary);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn check_hosted_records(root: &Path, coverage: &Value, revision: &str) -> Value {
    use serde_json::json;
    let files = [
        "rust-test-suite-report.json",
        "rust-test-suite-report.json.selections.json",
        "rust-test-suite-report.json.make-coverage.json",
    ];
    fs::create_dir(root.join("target/hosted")).unwrap();
    for name in files {
        fs::copy(
            root.join("target/quality").join(name),
            root.join("target/hosted").join(name),
        )
        .unwrap();
    }
    let context = json!({"schema":"terlan.hosted-download-cache.v1","repository":"fixture/repository",
        "revision":revision,
        "compiler":{"id":123456,"attempt":2,"path":".github/workflows/ci.yml"}});
    let context_path = "target/hosted-context.json";
    write(root, context_path, serde_json::to_vec(&context).unwrap());
    let invoke = || {
        ProcessControl::new(Duration::from_secs(30)).capture_stdout(
            command(root, root.join("target/driver")).args([
                "--check-hosted-coverage-records",
                "target/hosted",
                context_path,
            ]),
            1024 * 1024,
            |_| Ok(()),
        )
    };
    let before = fs::read(root.join("target/bodies.txt")).unwrap();
    let output = invoke().expect("valid real hosted records");
    let inspected: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(inspected["decision"], "records-consistent");
    assert_eq!(inspected["authentication_required"], true);
    assert_eq!(inspected["current_inputs_verified"], false);
    assert_eq!(inspected["reusable"], false);
    assert_eq!(inspected["files"].as_object().unwrap().len(), 3);
    for name in files {
        let digest = sha2::Sha256::digest(fs::read(root.join("target/hosted").join(name)).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(inspected["files"][name], digest);
    }
    for (pointer, replacement) in [
        ("/compiler/id", json!(123457)),
        ("/compiler/attempt", json!(1)),
        ("/compiler/path", json!(".github/workflows/release.yml")),
        ("/repository", json!("another/repository")),
        (
            "/revision",
            json!("1123456789012345678901234567890123456789"),
        ),
    ] {
        let mut changed = context.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        write(root, context_path, serde_json::to_vec(&changed).unwrap());
        assert!(
            invoke().is_err(),
            "accepted wrong expected context: {pointer}"
        );
    }
    write(root, context_path, serde_json::to_vec(&context).unwrap());
    let index = coverage["phases"]
        .as_array()
        .unwrap()
        .iter()
        .position(|phase| phase["executor"] == "make-covered-gates")
        .unwrap();
    let evidence = format!("/phases/{index}/test_execution");
    for (pointer, replacement) in [
        ("/decision".into(), json!("pass")),
        ("/direct_cargo_launch_count".into(), json!(1)),
        ("/source_binding/verified".into(), json!(false)),
        ("/source_binding/after/sha256".into(), json!("changed")),
        (
            format!("{evidence}/producer_context/host_arch"),
            json!("aarch64"),
        ),
        (format!("{evidence}/producer_context/job"), json!("another")),
        (
            format!("{evidence}/completed_suite_binding/sha256"),
            json!("0".repeat(64)),
        ),
        (
            format!("{evidence}/completed_suite_binding/run_id"),
            json!("other"),
        ),
        (
            format!("{evidence}/make_invocation_sha256"),
            json!("0".repeat(64)),
        ),
        (
            format!("{evidence}/make_executable_binding/verified"),
            json!(false),
        ),
        (format!("{evidence}/requester_process_count"), json!(0)),
        (format!("{evidence}/requests/0/decision"), json!("fail")),
        (format!("{evidence}/requests/0/pid"), json!(0)),
        (
            format!("{evidence}/requests/0/request/arguments"),
            json!(["-p", "terlan", "--lib", "missing::test"]),
        ),
        (
            format!("{evidence}/requests/0/request/arguments"),
            json!(["-p", "support", "--test", "missing"]),
        ),
        (format!("/phases/{index}/child_pid"), Value::Null),
    ] {
        let mut changed = coverage.clone();
        *changed.pointer_mut(&pointer).unwrap() = replacement;
        write(
            root,
            &format!("target/hosted/{}", files[2]),
            serde_json::to_vec(&changed).unwrap(),
        );
        assert!(invoke().is_err(), "accepted altered record: {pointer}");
    }
    fs::copy(
        root.join("target/quality").join(files[2]),
        root.join("target/hosted").join(files[2]),
    )
    .unwrap();
    let original = fs::read(root.join("target/hosted").join(files[0])).unwrap();
    let mut changed = original.clone();
    changed.push(b'\n');
    write(root, &format!("target/hosted/{}", files[0]), changed);
    assert!(invoke().is_err(), "accepted different suite bytes");
    write(root, &format!("target/hosted/{}", files[0]), original);
    write(root, "target/hosted/unexpected.json", b"{}");
    assert!(invoke().is_err(), "accepted extra artifact");
    fs::remove_file(root.join("target/hosted/unexpected.json")).unwrap();
    fs::remove_file(root.join("target/hosted").join(files[1])).unwrap();
    assert!(invoke().is_err(), "accepted missing selections");
    std::os::unix::fs::symlink(
        root.join("target/quality").join(files[1]),
        root.join("target/hosted").join(files[1]),
    )
    .unwrap();
    assert!(invoke().is_err(), "accepted symlinked selections");
    assert_eq!(fs::read(root.join("target/bodies.txt")).unwrap(), before);
    inspected
}
