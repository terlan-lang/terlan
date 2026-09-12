//! Execute the production preparation recipe with fixture-owned external services.
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Each service records its real invocation; repeated producers are errors.
#[test]
fn preparation_leaf() {
    let Some(root) = std::env::var_os("TERLAN_PREPARE_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let stage = std::env::var("TERLAN_PREPARE_STAGE").unwrap();
    let mode = std::env::var("TERLAN_PREPARE_MODE").unwrap();
    let fault = std::env::var("TERLAN_PREPARE_FAULT").unwrap();
    let hang_stage = std::env::var("TERLAN_PREPARE_HANG").unwrap_or_default();
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("events"))
        .unwrap()
        .write_all(format!("{stage}\n").as_bytes())
        .unwrap();
    if hang_stage == stage {
        let child = Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; sleep 300"])
            .spawn()
            .expect("hanging preparation descendant starts");
        let child_id = child.id();
        std::thread::spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
        fs::write(root.join("descendant"), child_id.to_string()).unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    let started = root.join(format!("started-{stage}"));
    if (mode == "warm" || mode == "resume") && started.is_file() {
        if root.join(format!("passed-{stage}")).is_file() {
            return;
        }
        assert_eq!(
            mode, "resume",
            "warm reuse observed an incomplete producer: {stage}"
        );
        fs::remove_file(&started).expect("interrupted producer marker is removable");
    }
    if stage == "publish-evidence-check" {
        assert!(
            mode == "warm" || root.join("passed-publish-evidence-refresh").is_file(),
            "cold evidence requires refresh"
        );
        assert_ne!(fault, "post-verification", "refreshed evidence rejected");
        return;
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&started)
        .expect("equivalent preparation producer ran twice");
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("producer-events"))
        .unwrap()
        .write_all(format!("{stage}\n").as_bytes())
        .unwrap();
    if stage == "distribution" || stage == "publish-evidence-refresh" {
        let dependency = if stage == "distribution" {
            "readiness"
        } else {
            "distribution"
        };
        assert!(root.join(format!("passed-{dependency}")).is_file());
    }
    if stage == "release-preflight" {
        for dependency in [
            "distribution",
            "release-boundary-check",
            "source-extension-check",
            "publish-cache-prune",
        ] {
            assert!(
                root.join(format!("passed-{dependency}")).is_file(),
                "{dependency}"
            );
        }
    }
    assert_ne!(stage, fault, "injected preparation service failure");
    fs::write(root.join(format!("passed-{stage}")), b"pass").unwrap();
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn successful_owner_outputs(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut outputs = fs::read_dir(root)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("passed-") {
                return None;
            }
            fs::read(entry.path()).ok().map(|bytes| (name, bytes))
        })
        .collect::<Vec<_>>();
    outputs.sort_by(|left, right| left.0.cmp(&right.0));
    outputs
}

fn descendant_stopped(root: &Path) {
    let pid: u32 = fs::read_to_string(root.join("descendant"))
        .expect("hanging descendant marker")
        .trim()
        .parse()
        .expect("hanging descendant pid");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Ok(stat) if stat.rsplit_once(") ").unwrap().1.starts_with('Z') => return,
            _ => assert!(
                std::time::Instant::now() < deadline,
                "full candidate descendant {pid} still running"
            ),
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-publish-prepare-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let fixture = Fixture(root);
    for directory in ["bin", "dist", "scripts"] {
        fs::create_dir(fixture.0.join(directory)).unwrap();
    }
    for tool in [
        "cargo",
        "rustc",
        "node",
        "npm",
        "jq",
        "lake",
        "clang",
        "flock",
        "sha256sum",
        "realpath",
        "timeout",
        "java",
    ] {
        executable(&fixture.0.join("bin").join(tool), "#!/bin/sh\nexit 0\n");
    }
    executable(
        &fixture.0.join("bin/cargo"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cargo 1.96.0'; fi\nexit 0\n",
    );
    executable(
        &fixture.0.join("bin/rustc"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'rustc 1.96.0'; fi\nexit 0\n",
    );
    fs::write(
        fixture.0.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.96.0\"\n",
    )
    .unwrap();
    executable(
        &fixture.0.join("bin/git"),
        "#!/bin/sh\ntest \"$*\" = 'rev-parse HEAD' || exit 1\nprintf 'fixture-revision\\n'\n",
    );
    executable(
        &fixture.0.join("dist/terlc"),
        "#!/bin/sh\ntest \"$1\" = --version\n",
    );
    executable(
        &fixture
            .0
            .join("scripts/download_validated_release_artifacts.sh"),
        "#!/bin/sh\ntest \"$1\" = fixture-revision\n",
    );
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let begin = source
        .find("\npublish-prepare: publish-source-preflight\n")
        .unwrap()
        + 1;
    let end = source[begin..]
        .find("\npublish-cache-prune: | terlan-release-promotion-bootstrap")
        .unwrap()
        + begin;
    let mut make = format!(
        "SHELL := /bin/bash\nVERSION := fixture\n{}\n",
        &source[begin..end]
    );
    let recipe =
        "\t@TERLAN_PREPARE_STAGE=$@ \"$(FIXTURE_CHILD)\" --exact preparation_leaf --nocapture\n";
    for stage in [
        "publish-source-preflight",
        "terlan-compiler-bootstrap",
        "release-version-metadata-check",
        "publish-evidence-plan-check",
        "publish-evidence-refresh-plan-check",
        "publish-evidence-check",
        "release-boundary-check",
        "source-extension-check",
        "terlan-release-promotion-bootstrap",
        "publish-preparation-admission",
        "publish-cache-prune",
        "release-preflight",
    ] {
        make.push_str(&format!(".PHONY: {stage}\n{stage}:\n{recipe}"));
    }
    // Model the existing enclosing graph and focused repair with distinct Make
    // nodes but the same observed readiness/installation producer identities.
    make.push_str(&format!(
        "publish-evidence-refresh: release-version-metadata-check terlan-compiler-bootstrap release-staged-distribution-verification-check\n{recipe}"
    ));
    make.push_str("release-version-metadata-check source-extension-check terlan-release-promotion-bootstrap: terlan-compiler-bootstrap\n");
    for suffix in ["check", "refresh"] {
        make.push_str(&format!(
            "release-readiness-attestation-{suffix}:\n\t@TERLAN_PREPARE_STAGE=readiness \"$(FIXTURE_CHILD)\" --exact preparation_leaf --nocapture\nrelease-staged-distribution-verification-{suffix}: release-readiness-attestation-{suffix}\n\t@TERLAN_PREPARE_STAGE=distribution \"$(FIXTURE_CHILD)\" --exact preparation_leaf --nocapture\n"
        ));
    }
    make.push_str(
        "publish-staged-distribution-prepare: release-staged-distribution-verification-refresh\n",
    );
    fs::write(fixture.0.join("Makefile"), make).unwrap();
    fixture
}

fn fresh_fixture() -> Fixture {
    fixture()
}

#[test]
fn cold_preparation_does_not_replay_distribution_and_warm_repair_still_runs() {
    for (mode, fault) in [
        ("cold", ""),
        ("warm", ""),
        ("cold", "publish-evidence-refresh"),
        ("cold", "readiness"),
        ("cold", "distribution"),
        ("cold", "post-verification"),
        ("warm", "readiness"),
        ("warm", "distribution"),
        ("cold", "release-boundary-check"),
        ("warm", "release-boundary-check"),
        ("cold", "release-preflight"),
        ("cold", "publish-source-preflight"),
        ("cold", "terlan-compiler-bootstrap"),
        ("warm", "terlan-compiler-bootstrap"),
        ("cold", "release-version-metadata-check"),
        ("warm", "release-version-metadata-check"),
    ] {
        let fixture = fixture();
        let mut paths = vec![fixture.0.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k", "publish-prepare"])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env_remove("JAVA_HOME")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
            .env("TERLAN_PREPARE_FIXTURE", &fixture.0)
            .env("TERLAN_PREPARE_MODE", mode)
            .env("TERLAN_PREPARE_FAULT", fault)
            // Caller state must never determine whether either path executes.
            .env(
                "terlan_repair_distribution",
                if mode == "cold" { "1" } else { "0" },
            );
        let result = ProcessControl::new(Duration::from_secs(120))
            .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
            .unwrap();
        let stdout = String::from_utf8(result.stdout).unwrap();
        assert_eq!(
            result.outcome.is_ok(),
            fault.is_empty(),
            "{mode}/{fault}: {stdout}"
        );
        let events = fs::read_to_string(fixture.0.join("events")).unwrap();
        let count = |stage: &str| events.lines().filter(|line| *line == stage).count();
        assert!(
            count("readiness") <= 1 && count("distribution") <= 1,
            "{events}"
        );
        if fault.is_empty() {
            assert_eq!(count("terlan-compiler-bootstrap"), 1, "{events}");
            assert_eq!(count("release-version-metadata-check"), 1, "{events}");
            assert_eq!(count("readiness"), 1, "{events}");
            assert_eq!(count("distribution"), 1, "{events}");
            assert_eq!(
                count("publish-evidence-check"),
                if mode == "cold" { 2 } else { 1 }
            );
            assert_eq!(
                count("publish-evidence-refresh"),
                usize::from(mode == "cold")
            );
            assert_eq!(count("release-preflight"), 1);
        } else if fault != "release-preflight" {
            assert_eq!(
                count("release-preflight"),
                0,
                "failed owner reached final preflight: {events}"
            );
        }
        // Independent source checks may now fail early alongside cold refresh.
        // Cache retirement and final promotion still require successful evidence.
        if fault == "post-verification" || fault == "publish-evidence-refresh" {
            assert_eq!(count("publish-cache-prune"), 0, "{events}");
        }
        if mode == "warm" && fault == "release-boundary-check" {
            assert_eq!(count("distribution"), 0, "{events}");
        }
    }
}

#[test]
fn preparation_rejects_competing_cold_and_warm_graphs_before_producers() {
    let fixture = fixture();
    let mut command = Command::new("/bin/sh");
    command
        .current_dir(&fixture.0)
        .args([
            "-c",
            "exec make \"$@\" 2>&1",
            "make",
            "--no-print-directory",
            "-j8",
            "publish-preparation-cold",
            "publish-preparation-warm",
        ])
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS");
    let result = ProcessControl::new(Duration::from_secs(10))
        .capture_stdout_result(&mut command, 64 * 1024, |_| Ok(()))
        .unwrap();
    assert!(result.outcome.is_err());
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("preparation selects either the cold or warm graph, never both"));
    assert!(!fixture.0.join("events").exists());
}

#[test]
fn unchanged_warm_preparation_reuses_the_same_candidate_without_producer_launches() {
    let fixture = fixture();
    let run = |mode: &str| {
        let mut paths = vec![fixture.0.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k", "publish-prepare"])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env_remove("JAVA_HOME")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
            .env("TERLAN_PREPARE_FIXTURE", &fixture.0)
            .env("TERLAN_PREPARE_MODE", mode)
            .env("TERLAN_PREPARE_FAULT", "");
        ProcessControl::new(Duration::from_secs(120))
            .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
            .unwrap()
    };

    assert!(run("cold").outcome.is_ok());
    let first_producers = fs::read_to_string(fixture.0.join("producer-events")).unwrap();
    assert!(!first_producers.is_empty());
    let first_outputs = successful_owner_outputs(&fixture.0);
    assert!(!first_outputs.is_empty());
    assert!(run("warm").outcome.is_ok());
    assert_eq!(
        fs::read_to_string(fixture.0.join("producer-events")).unwrap(),
        first_producers
    );
    assert_eq!(successful_owner_outputs(&fixture.0), first_outputs);
}

#[test]
fn full_candidate_rehearsal_covers_cold_warm_resume_and_upload_retry() {
    let fixture = fixture();
    let make_path = fixture.0.join("Makefile");
    let mut make = fs::read_to_string(&make_path).unwrap();
    make.push_str(
        r#"
.PHONY: publish publish-preflight publish-release-from-dist
publish: publish-preflight
	@$(MAKE) --no-print-directory publish-release-from-dist
publish-preflight:
	@printf 'verify\n' >> "$(FIXTURE_PUBLISH_LOG)"
publish-release-from-dist:
	@printf 'upload\n' >> "$(FIXTURE_PUBLISH_LOG)"
	@if [ ! -e "$(FIXTURE_UPLOAD_FAILED)" ]; then : > "$(FIXTURE_UPLOAD_FAILED)"; exit 1; fi
	@printf 'promotion\n' >> "$(FIXTURE_PUBLISH_LOG)"
"#,
    );
    fs::write(&make_path, make).unwrap();

    let run_preparation = |mode: &str, fault: &str| {
        let mut paths = vec![fixture.0.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k", "publish-prepare"])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env_remove("JAVA_HOME")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
            .env("TERLAN_PREPARE_FIXTURE", &fixture.0)
            .env("TERLAN_PREPARE_MODE", mode)
            .env("TERLAN_PREPARE_FAULT", fault);
        ProcessControl::new(Duration::from_secs(120))
            .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
            .unwrap()
    };

    assert!(run_preparation("cold", "").outcome.is_ok());
    let producers = fs::read_to_string(fixture.0.join("producer-events")).unwrap();
    assert!(!producers.is_empty());
    let outputs = successful_owner_outputs(&fixture.0);
    assert!(!outputs.is_empty());

    assert!(run_preparation("warm", "").outcome.is_ok());
    assert_eq!(
        fs::read_to_string(fixture.0.join("producer-events")).unwrap(),
        producers
    );
    assert_eq!(successful_owner_outputs(&fixture.0), outputs);

    // Interrupt the final preparation owner, then resume it without replaying
    // any completed owner or changing its sealed output bytes.
    assert!(run_preparation("cold", "release-preflight")
        .outcome
        .is_err());
    assert!(run_preparation("resume", "").outcome.is_ok());
    assert_eq!(
        fs::read_to_string(fixture.0.join("producer-events")).unwrap(),
        producers
    );
    assert_eq!(successful_owner_outputs(&fixture.0), outputs);

    let publish_log = fixture.0.join("publish-operations");
    let upload_failed = fixture.0.join("upload-failed");
    let run_publish = |log: &Path, failed: &Path| {
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "publish"])
            .env("FIXTURE_PUBLISH_LOG", log)
            .env("FIXTURE_UPLOAD_FAILED", failed)
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS");
        ProcessControl::new(Duration::from_secs(30)).run(&mut command, |_| Ok(()))
    };
    assert!(run_publish(&publish_log, &upload_failed).is_err());
    assert!(run_publish(&publish_log, &upload_failed).is_ok());
    assert_eq!(
        fs::read_to_string(&publish_log).unwrap(),
        "verify\nupload\nverify\nupload\npromotion\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.0.join("producer-events")).unwrap(),
        producers,
        "publication retry must not rerun preparation producers"
    );

    // Exercise descendant containment on the same full-candidate path. A
    // separate process-owner unit test is not enough: preparation must also
    // reap work started by a candidate producer when the enclosing owner times
    // out.
    let containment_fixture = fresh_fixture();
    let mut paths = vec![containment_fixture.0.join("bin")];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let mut command = Command::new("make");
    command
        .current_dir(&containment_fixture.0)
        .args(["--no-print-directory", "-j8", "-k", "publish-prepare"])
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS")
        .env_remove("JAVA_HOME")
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
        .env("TERLAN_PREPARE_FIXTURE", &containment_fixture.0)
        .env("TERLAN_PREPARE_MODE", "cold")
        .env("TERLAN_PREPARE_FAULT", "")
        .env("TERLAN_PREPARE_HANG", "terlan-compiler-bootstrap");
    let result = ProcessControl::new(Duration::from_secs(3)).capture_stdout_result(
        &mut command,
        256 * 1024,
        |_| Ok(()),
    );
    assert!(result.is_err(), "hanging candidate must time out");
    descendant_stopped(&containment_fixture.0);
    println!("full candidate containment rehearsal passed: hanging descendant reaped");
}

#[test]
fn mismatched_rust_channel_is_rejected_before_preparation_producers() {
    let fixture = fixture();
    let before = fs::read_to_string(fixture.0.join("producer-events")).unwrap_or_default();
    fs::write(
        fixture.0.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.95.0\"\n",
    )
    .unwrap();
    let mut paths = vec![fixture.0.join("bin")];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let mut command = Command::new("make");
    command
        .current_dir(&fixture.0)
        .args(["--no-print-directory", "-j8", "-k", "publish-prepare"])
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS")
        .env_remove("JAVA_HOME")
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
        .env("TERLAN_PREPARE_FIXTURE", &fixture.0)
        .env("TERLAN_PREPARE_MODE", "cold")
        .env("TERLAN_PREPARE_FAULT", "");
    let result = ProcessControl::new(Duration::from_secs(120))
        .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
        .unwrap();
    assert!(result.outcome.is_err());
    let producers = fs::read_to_string(fixture.0.join("producer-events")).unwrap_or_default();
    assert_eq!(producers, format!("{before}publish-source-preflight\n"));
}

#[test]
fn preparation_network_and_binary_probes_have_outer_deadlines() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\npublish-prepare: publish-source-preflight\n")
        .unwrap()
        + 1;
    let end = source[start..].find("\n# One Make process owns").unwrap() + start;
    let recipe = &source[start..end];
    assert!(recipe.contains("timeout 900s bash scripts/download_validated_release_artifacts.sh"));
    assert!(recipe.contains("timeout 30s dist/terlc --version"));
    assert!(source.contains("timeout 900s $(TERLAN_RELEASE_PROMOTION) prepare-release-reports"));
    assert!(source.contains("timeout 900s $(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-matrix"));
    assert!(
        source.contains("timeout 1800s $(TERLAN_RUST_ORCHESTRATOR) --with-hosted-cargo-coverage")
    );
    assert!(
        !recipe.contains("gh auth status"),
        "local preparation must not require GitHub authentication"
    );
    assert!(
        source.contains("publish-remote-preflight: publish-source-preflight")
            && source.contains("gh auth status"),
        "publication-only preflight must retain the authentication gate"
    );
}

#[test]
fn clean_bootstrap_uses_one_receipt_owner_and_dirty_bootstrap_falls_back_safely() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\nterlan-compiler-bootstrap:\n")
        .expect("compiler bootstrap target is declared")
        + 1;
    let end = source[start..]
        .find("\n# Seal the quality")
        .expect("compiler bootstrap target has a boundary")
        + start;
    let recipe = &source[start..end];
    assert!(recipe.contains("git diff --quiet"));
    assert!(recipe.contains("git diff --cached --quiet"));
    assert!(recipe.contains("git ls-files --others --exclude-standard"));
    assert!(recipe.contains("\"$(TERLAN_BUILD_CACHE)\" owner"));
    assert!(recipe.contains("--receipt \"target/quality/preparation/bootstrap/"));
    assert!(recipe.contains("--output \"$(TERLAN_BOOTSTRAP_COMPILER)\""));
    assert!(recipe.contains("--output \"$(TERLAN_BOOTSTRAP_VM)\""));
    assert!(recipe.contains("--output \"$(patsubst $(CURDIR)/%,%,$(TERLAN_RUST_ORCHESTRATOR))\""));
    assert!(recipe.contains("--run-owned --timeout-seconds"));
}

#[test]
fn aot_release_default_feature_check_records_an_owned_cargo_launch() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    assert!(source.contains(
        "AOT_RELEASE_CARGO_CHECK := $(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds"
    ));
    assert!(source.contains("-- env -u RUSTFLAGS $(CARGO) check -p terlan"));
}

#[test]
fn hosted_proof_scripts_use_private_owner_outputs() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    for (stage, variable, script) in [
        ("smoke", "SMOKE", "LeanProofSmokeTest"),
        ("lanes", "LANES", "LeanProofLanesTest"),
        (
            "native-boundary",
            "NATIVE_BOUNDARY",
            "LeanProofNativeBoundaryTest",
        ),
    ] {
        assert!(source.contains(&format!(
            "TERLAN_OWNED_PROOF_{variable} = $(TERLAN_PREPARATION_OWNER)"
        )));
        assert!(source.contains(&format!("prepare-proof-{stage} \"target/debug/terlc\"")));
        assert!(source.contains(&format!(
            "lean-proof-{stage}-check: TERLAN_PROOF_{variable}_RUN = $(TERLAN_OWNED_PROOF_{variable})"
        )));
        assert!(source.contains(&format!(
            "lean-proof-{stage}-check: | terlan-release-promotion-bootstrap publish-preparation-lock-directory"
        )));
        assert!(source.contains(&format!(
            "TERLAN_PROOF_{variable}_RUN = target/debug/terlc test --incremental scripts/self_validation/{script}.terl"
        )));
    }
    assert!(source.contains("lean-proof-native-boundary-check: proof-repro-check"));
    assert!(source.contains(
        "publish-evidence-staged-inputs: publish-evidence-source-prerequisites lean-proof-lanes-check"
    ));
}

#[test]
fn every_typed_aot_release_producer_declares_the_compiler_owner() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let producers = [
        "terlan-artifact-measurement-bootstrap",
        "terlan-make-recipe-bootstrap",
        "terlan-semantic-kernel-bootstrap",
        "terlan-ebnf-validator-bootstrap",
        "terlan-shared-helper-bootstrap",
        "terlan-external-package-matrix-bootstrap",
        "terlan-tvm-package-consumer-bootstrap",
        "terlan-tvm-platform-matrix-bootstrap",
        "terlan-rust-quality-bootstrap",
        "terlan-release-promotion-bootstrap",
        "terlan-release-closeout-image-bootstrap",
        "terlan-web-manifest-preflight-bootstrap",
        "terlan-self-validation-checkout-bootstrap",
        "terlan-stdlib-validation-bootstrap",
        "terlan-docs-static-release-parity-bootstrap",
        "terlan-repository-validation-bootstrap",
        "terlan-proof-release-bootstrap",
    ];
    for producer in producers {
        let declaration = format!("{producer}: terlan-compiler-bootstrap");
        assert!(
            source.contains(&declaration),
            "typed AOT producer {producer} must wait for the compiler owner"
        );
    }
}

/// Execute the production dependency edges with fixture-owned leaf commands.
#[test]
fn hosted_native_proof_failure_blocks_parallel_distribution() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let stages = [
        "publish-evidence-source-prerequisites",
        "lean-proof-track-runtime-check",
        "proof-repro-check",
        "lean-proof-native-boundary-check",
        "lean-proof-smoke-check",
        "lean-proof-track-pr-gate",
        "lean-proof-track-regression-check",
        "lean-proof-semantic-kernels-check",
        "lean-proof-lanes-check",
        "publish-evidence-staged-inputs",
    ];
    let mut graph = format!(".PHONY: {}\n", stages.join(" "));
    for line in source.lines() {
        let Some((target, dependencies)) = line.split_once(':') else {
            continue;
        };
        if !stages.contains(&target) || dependencies.contains('=') {
            continue;
        }
        let selected = dependencies
            .split_whitespace()
            .take_while(|value| *value != "|")
            .filter(|value| stages.contains(value))
            .collect::<Vec<_>>();
        graph.push_str(&format!("{target}: {}\n", selected.join(" ")));
    }
    for stage in stages {
        let prerequisite = match stage {
            "lean-proof-native-boundary-check" => Some("proof-repro-check"),
            "lean-proof-smoke-check" => Some("lean-proof-native-boundary-check"),
            "lean-proof-lanes-check" => Some("lean-proof-smoke-check"),
            "publish-evidence-staged-inputs" => Some("lean-proof-lanes-check"),
            _ => None,
        };
        graph.push_str(&format!("{stage}:\n"));
        if let Some(prerequisite) = prerequisite {
            graph.push_str(&format!("\ttest -s passed-{prerequisite}\n"));
        }
        graph.push_str(&format!(
            "\ttest \"$(FAULT)\" != \"{stage}\"\n\tprintf 'pass\\n' > passed-{stage}\n"
        ));
    }
    for fault in ["", "lean-proof-native-boundary-check"] {
        let fixture = fixture();
        fs::write(fixture.0.join("Proof.mk"), &graph).unwrap();
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args([
                "--no-print-directory",
                "-f",
                "Proof.mk",
                "-j8",
                "-k",
                "publish-evidence-staged-inputs",
                "lean-proof-lanes-check",
            ])
            .arg(format!("FAULT={fault}"))
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS");
        let result = ProcessControl::new(Duration::from_secs(30))
            .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(result.outcome.is_ok(), fault.is_empty());
        assert_eq!(
            fixture
                .0
                .join("passed-publish-evidence-staged-inputs")
                .is_file(),
            fault.is_empty(),
            "distribution must wait for all proof consumers"
        );
    }
}

#[test]
fn owner_graph_closeout_has_one_entry_point_for_every_rehearsal() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("release-preparation-owner-graph-check:")
        .expect("owner graph closeout target is declared");
    let end = source[start..]
        .find("\n\n")
        .map(|offset| start + offset)
        .unwrap_or(source.len());
    let recipe = &source[start..end];
    for target in [
        "release-preparation-recovery-check",
        "release-preparation-contract-check",
        "release-preparation-aot-check",
        "release-preparation-release-reports-check",
        "release-preparation-local-reports-check",
        "release-preparation-multicore-check",
        "release-preparation-readiness-check",
        "release-preparation-staged-distribution-check",
        "release-preparation-proof-check",
        "release-preparation-proof-smoke-check",
        "release-preparation-proof-native-boundary-check",
        "release-preparation-proof-lanes-check",
    ] {
        assert!(
            recipe.contains(target),
            "owner graph closeout must include {target}"
        );
    }
}

#[test]
fn quality_tools_bootstrap_uses_shared_receipt_owner_for_clean_candidates() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\nterlan-quality-tools-bootstrap:")
        .expect("quality-tools bootstrap target is declared")
        + 1;
    let end = source[start..]
        .find("\nterlan-benchmark-release-bootstrap:")
        .expect("quality-tools bootstrap target has a boundary")
        + start;
    let recipe = &source[start..end];
    assert!(recipe.contains("terlan-quality-tools-bootstrap: | terlan-build-owner-bootstrap"));
    assert!(recipe.contains("git diff --quiet"));
    assert!(recipe.contains("git diff --cached --quiet"));
    assert!(recipe.contains("git ls-files --others --exclude-standard"));
    assert!(recipe.contains("\"$(TERLAN_BUILD_CACHE)\" owner"));
    assert!(recipe.contains("quality-tools.json"));
    for binary in [
        "terlan-quality",
        "terlan-native-target-feasibility",
        "terlan-lean-proof-closeout",
        "terlan-rust-boundary-audit",
    ] {
        assert!(
            recipe.contains(&format!("--output target/debug/{binary}")),
            "quality-tools owner must bind {binary}"
        );
    }
    assert!(recipe.contains("--run-owned --timeout-seconds"));
}

#[test]
fn benchmark_release_bootstrap_binds_all_release_binaries_to_one_owner() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\nterlan-benchmark-release-bootstrap:")
        .expect("benchmark bootstrap target is declared")
        + 1;
    let end = source[start..]
        .find("\n# GNU Make 4.3")
        .expect("benchmark bootstrap target has a boundary")
        + start;
    let recipe = &source[start..end];
    assert!(recipe.contains("terlan-benchmark-release-bootstrap: | terlan-build-owner-bootstrap"));
    assert!(recipe.contains("benchmark-release.json"));
    for binary in ["terlc", "terlan-vm", "terlan-benchmark"] {
        assert!(
            recipe.contains(&format!("--output target/release/{binary}")),
            "benchmark owner must bind target/release/{binary}"
        );
    }
    assert!(recipe.contains("--run-owned --timeout-seconds"));
}

#[test]
fn http_benchmark_bootstrap_binds_all_comparison_binaries_to_one_owner() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\nterlan-http-benchmark-release-bootstrap:")
        .expect("HTTP benchmark bootstrap target is declared")
        + 1;
    let end = source[start..]
        .find("\ndocs-light-check:")
        .expect("HTTP benchmark bootstrap target has a boundary")
        + start;
    let recipe = &source[start..end];
    assert!(
        recipe.contains("terlan-http-benchmark-release-bootstrap: | terlan-build-owner-bootstrap")
    );
    assert!(recipe.contains("http-benchmark-release.json"));
    for binary in [
        "terlan-axum-baseline",
        "terlan-hyper-baseline",
        "terlan-http-framework-benchmark",
        "terlan-http-paired-benchmark",
    ] {
        assert!(
            recipe.contains(&format!("--output target/release/{binary}")),
            "HTTP benchmark owner must bind target/release/{binary}"
        );
    }
    assert!(recipe.contains("--run-owned --timeout-seconds"));
}

#[test]
fn serve_runtime_bootstrap_binds_profile_binary_to_one_owner() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\nterlan-serve-runtime-bootstrap:")
        .expect("serve runtime bootstrap target is declared")
        + 1;
    let end = source[start..]
        .find("\nifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)")
        .expect("serve runtime bootstrap target has a boundary")
        + start;
    let recipe = &source[start..end];
    assert!(recipe.contains("terlan-serve-runtime-bootstrap: | terlan-build-owner-bootstrap"));
    assert!(recipe.contains("serve-runtime.json"));
    assert!(recipe.contains("TERLAN_SERVE_RUNTIME_REL_BIN"));
    assert!(recipe.contains("\"$(TERLAN_BUILD_CACHE)\" owner"));
    assert!(recipe.contains("--run-owned --timeout-seconds"));
}

#[test]
fn preparation_always_runs_locked_publication_cache_retirement() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    for mode in ["cold", "warm"] {
        let target = format!("publish-preparation-{mode}:");
        let line = source
            .lines()
            .find(|line| line.starts_with(&target))
            .unwrap_or_else(|| panic!("missing {target} target"));
        assert!(
            line.split_whitespace()
                .any(|dependency| dependency == "publish-cache-prune"),
            "{mode} preparation must retire obsolete publication cache generations"
        );
    }
    let start = source
        .find("\npublish-cache-prune: | terlan-release-promotion-bootstrap\n")
        .unwrap()
        + 1;
    let end = source[start..].find("\n\n# This native bootstrap").unwrap() + start;
    let recipe = &source[start..end];
    assert!(recipe.contains("flock --exclusive --nonblock target/publication-inputs.lock"));
    assert!(recipe.contains("timeout 120s $(TERLAN_RELEASE_PROMOTION) prune-publication-cache"));
}

#[test]
fn preparation_branches_share_one_resource_admission() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let prepare_start = source
        .find("\npublish-prepare: publish-source-preflight\n")
        .expect("publish preparation entry point is declared");
    let prepare_end = source[prepare_start..]
        .find("\n# One Make process owns")
        .expect("publish preparation entry point has a boundary")
        + prepare_start;
    let prepare = &source[prepare_start..prepare_end];
    assert!(prepare.contains("exec 9>target/quality/preparation.lock"));
    assert!(prepare.contains("timeout 120s flock -w 120 9"));
    assert!(prepare.contains("export TERLAN_PREPARATION_LOCK_HELD=1"));
    let lock_position = prepare
        .find("timeout 120s flock -w 120 9")
        .expect("preparation lease is acquired");
    let branch_position = prepare
        .find("if $(MAKE) --no-print-directory publish-evidence-check")
        .expect("preparation branch selection is declared");
    let final_preflight_position = prepare
        .find("$(MAKE) release-preflight RELEASE_VERSION=\"$(VERSION)\"")
        .expect("final preflight is declared");
    assert!(lock_position < branch_position);
    assert!(branch_position < final_preflight_position);
    let checks = source
        .lines()
        .find(|line| line.starts_with("publish-preparation-checks:"))
        .expect("publication checks target is declared");
    assert!(
        checks
            .split_whitespace()
            .any(|dependency| dependency == "publish-preparation-admission"),
        "both preparation branches must pass through shared admission"
    );
    let admission = source
        .lines()
        .find(|line| line.starts_with("publish-preparation-admission:"))
        .expect("shared admission target is declared");
    assert!(admission.contains("rust-build-resource-admission"));
    let refresh = source
        .lines()
        .find(|line| line.starts_with("publish-evidence-refresh:"))
        .expect("evidence refresh target is declared");
    assert!(
        refresh
            .split_whitespace()
            .any(|dependency| dependency == "publish-preparation-admission"),
        "the canonical refresh owner must use shared admission"
    );
    let covered = source
        .lines()
        .find(|line| line.starts_with("publish-evidence-covered-gates:"))
        .expect("covered-gates target is declared");
    assert!(
        covered
            .split_whitespace()
            .any(|dependency| dependency == "lean-proof-lanes-check"),
        "the canonical refresh owner must include smoke/lane/native-boundary gates"
    );
    let resource_start = source
        .find("\nrust-build-resource-admission: rust-build-cache-bootstrap\n")
        .expect("resource admission recipe is declared")
        + 1;
    let resource_end = source[resource_start..]
        .find("\n\nrust-incremental-cache-audit:")
        .expect("resource admission recipe has a boundary")
        + resource_start;
    assert!(
        source[resource_start..resource_end].contains("$(TERLAN_PREPARATION_OWNER) timeout 120s")
    );
}

#[test]
fn interrupted_preparation_resumes_only_the_failed_owner() {
    let fixture = fixture();
    let run = |mode: &str, fault: &str| {
        let mut paths = vec![fixture.0.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k", "publish-prepare"])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env_remove("JAVA_HOME")
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("FIXTURE_CHILD", std::env::current_exe().unwrap())
            .env("TERLAN_PREPARE_FIXTURE", &fixture.0)
            .env("TERLAN_PREPARE_MODE", mode)
            .env("TERLAN_PREPARE_FAULT", fault);
        ProcessControl::new(Duration::from_secs(120))
            .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
            .unwrap()
    };

    assert!(
        run("cold", "release-preflight").outcome.is_err(),
        "final owner fault must interrupt preparation"
    );
    let after_failure = fs::read_to_string(fixture.0.join("producer-events")).unwrap();
    assert!(after_failure
        .lines()
        .any(|stage| stage == "release-preflight"));
    assert!(
        run("resume", "").outcome.is_ok(),
        "resume should recover the candidate"
    );
    let after_resume = fs::read_to_string(fixture.0.join("producer-events")).unwrap();
    let count = |events: &str, stage: &str| events.lines().filter(|line| *line == stage).count();
    for stage in [
        "terlan-compiler-bootstrap",
        "release-version-metadata-check",
        "readiness",
        "distribution",
        "publish-evidence-refresh",
    ] {
        assert_eq!(
            count(&after_resume, stage),
            count(&after_failure, stage),
            "{stage}"
        );
    }
    assert_eq!(count(&after_resume, "release-preflight"), 2);
}

/// Cold hosted and warm publication paths select the same checkpoint command.
#[test]
fn distribution_routing_preserves_standalone_checks_and_candidate_failure() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source.find("\nTERLAN_STAGED_DISTRIBUTION_RUN =").unwrap() + 1;
    let end = source[start..]
        .find("\nterlan-web-manifest-preflight-bootstrap:")
        .unwrap()
        + start;
    let hosted = source
        .lines()
        .find(|line| {
            line.starts_with(
                "release-staged-distribution-verification-check: TERLAN_STAGED_DISTRIBUTION_RUN =",
            )
        })
        .unwrap();
    for (goal, is_hosted, fault, owned) in [
        (
            "release-staged-distribution-verification-check",
            false,
            "",
            false,
        ),
        (
            "release-staged-distribution-verification-refresh",
            false,
            "",
            false,
        ),
        ("publish-staged-distribution-prepare", false, "", true),
        (
            "release-staged-distribution-verification-check",
            true,
            "",
            true,
        ),
        ("publish-staged-distribution-prepare", false, "verify", true),
        (
            "release-staged-distribution-verification-check",
            true,
            "verify",
            true,
        ),
        (
            "publish-staged-distribution-prepare",
            false,
            "prepare-staged-distribution",
            true,
        ),
    ] {
        let fixture = fixture();
        fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
        fs::write(fixture.0.join("matrix.tvm"), b"fixture image").unwrap();
        let service = fixture.0.join("distribution-service");
        executable(
            &service,
            r#"#!/bin/sh
set -eu
printf '%s\n' "$1" >> events
test "$1" != "$TERLAN_ROUTING_FAULT" || exit 9
if [ "$1" != verify ]; then
    printf '%s\n' '{"decision": "pass", "failed_upgrade_rollback": "pass", "source_checkout_required": false}' > target/quality/release-staged-distribution-verification-report.json
fi
"#,
        );
        let mut make = format!(
            "SHELL := /bin/bash\nTERLAN_RELEASE_PROMOTION := {}\nTERLAN_TVM_PLATFORM_MATRIX := {}\nTERLAN_TVM_PLATFORM_MATRIX_IMAGE := matrix.tvm\nTERLAN_BOOTSTRAP_VM := fixture-vm\nRELEASE_VERSION := fixture\n{}\nrelease-readiness-attestation-check release-readiness-attestation-refresh terlan-tvm-platform-matrix-bootstrap terlan-release-promotion-bootstrap:\n\t@true\n",
            service.display(),
            service.display(),
            &source[start..end]
        );
        if is_hosted {
            make.push_str(hosted);
            make.push('\n');
        }
        fs::write(fixture.0.join("Makefile"), make).unwrap();
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k", goal])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env("TERLAN_ROUTING_FAULT", fault);
        let output = ProcessControl::new(Duration::from_secs(30))
            .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(
            output.outcome.is_ok(),
            fault.is_empty(),
            "{goal}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let events = fs::read_to_string(fixture.0.join("events")).unwrap();
        if fault == "verify" {
            assert_eq!(events, "verify\n");
        } else {
            assert_eq!(
                events,
                if owned {
                    "verify\nprepare-staged-distribution\n"
                } else {
                    "verify\nrelease-staged-distribution\n"
                }
            );
        }
    }
}

#[test]
fn readiness_routing_uses_one_owned_producer_and_stops_on_failure() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source.find("\nTERLAN_READINESS_RUN =").unwrap() + 1;
    let end = source[start..]
        .find("\nTERLAN_STAGED_DISTRIBUTION_RUN =")
        .unwrap()
        + start;
    for (goal, hosted, fault, owned) in [
        ("release-readiness-attestation-check", false, false, false),
        ("release-readiness-attestation-refresh", false, false, false),
        ("publish-staged-distribution-prepare", false, false, true),
        ("release-readiness-attestation-check", true, false, true),
        ("publish-staged-distribution-prepare", false, true, true),
        ("release-readiness-attestation-check", true, true, true),
    ] {
        let fixture = fixture();
        fs::create_dir_all(fixture.0.join("target/quality")).unwrap();
        fs::write(fixture.0.join("promotion.tvm"), b"fixture image").unwrap();
        executable(&fixture.0.join("fixture-vm"), "#!/bin/sh\nexit 99\n");
        let service = fixture.0.join("readiness-service");
        executable(
            &service,
            r#"#!/bin/sh
set -eu
printf '%s\n' "$1" >> events
test "$TERLAN_READINESS_FAULT" = 0 || exit 9
printf '%s\n' '{"decision": "pass", "publication_required": false}' > target/quality/release-readiness-attestation-report.json
"#,
        );
        let mut make = format!(
            "SHELL := /bin/bash\nTERLAN_RELEASE_PROMOTION := {}\nTERLAN_RELEASE_PROMOTION_IMAGE := promotion.tvm\nTERLAN_BOOTSTRAP_VM := fixture-vm\nRELEASE_VERSION := fixture\n{}\nrelease-example-projects-check release-project-upgrade-matrix-check release-reference-app-suite-check release-notes-accuracy-check release-fault-injection-check terlan-release-promotion-bootstrap:\n\t@true\npublish-staged-distribution-prepare: release-readiness-attestation-refresh\nfixture-accepted: {goal}\n\t@touch accepted\n",
            service.display(),
            &source[start..end]
        );
        for line in source.lines().filter(|line| {
            line.starts_with("publish-staged-distribution-prepare: TERLAN_READINESS_RUN =")
                || (hosted
                    && line
                        .starts_with("release-readiness-attestation-check: TERLAN_READINESS_RUN ="))
        }) {
            make.push_str(line);
            make.push('\n');
        }
        fs::write(fixture.0.join("Makefile"), make).unwrap();
        let mut command = Command::new("make");
        command
            .current_dir(&fixture.0)
            .args(["--no-print-directory", "-j8", "-k", "fixture-accepted"])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env("TERLAN_READINESS_FAULT", if fault { "1" } else { "0" });
        let output = ProcessControl::new(Duration::from_secs(30))
            .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(
            output.outcome.is_ok(),
            !fault,
            "{goal}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fixture.0.join("accepted").exists(), !fault);
        assert_eq!(
            fs::read_to_string(fixture.0.join("events")).unwrap(),
            if owned {
                "prepare-readiness\n"
            } else {
                "readiness\n"
            }
        );
    }
}
