//! Execute the production refresh-plan validator against bounded synthetic plans.
#![cfg(unix)]

use std::fs;
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

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "terlan-cold-plan-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    Fixture(root)
}

/// Planning must work before any build owner or compiler exists.
#[test]
fn cold_refresh_plan_needs_no_prebuilt_executables() {
    let fixture = fixture();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        "Makefile",
        "Cargo.toml",
        "mk/rust-coverage.mk",
        "mk/code-quality.mk",
        "crates/terlan/cli.mk",
        "std/stdlib.mk",
        "editors/editor.mk",
        "tree-sitter-terlan/package.json",
        "tree-sitter-terlan/package-lock.json",
    ] {
        let target = fixture.0.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repository.join(path), target).unwrap();
    }
    let mut command = Command::new("make");
    command.current_dir(&fixture.0).args([
        "--no-print-directory",
        "publish-evidence-plan-check",
        "publish-evidence-refresh-plan-check",
    ]);
    for name in ["MAKEFLAGS", "MAKEOVERRIDES", "MFLAGS"] {
        command.env_remove(name);
    }
    let output = ProcessControl::new(Duration::from_secs(30))
        .capture_stdout_result(&mut command, 256 * 1024, |_| Ok(()))
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(output.outcome.is_ok(), "{text}");
    assert!(text.contains("zero build and test replays"), "{text}");
    assert!(text.contains("duplicate-builds=0"), "{text}");
    assert!(
        !fixture.0.join("target").exists(),
        "planning launched a producer"
    );
}

/// Long option names containing n must not impersonate GNU Make's dry-run flag.
#[test]
fn executing_refresh_still_requires_live_coverage_owner() {
    let fixture = fixture();
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source.find("\npublish-evidence-refresh:").unwrap() + 1;
    let recipe = &source[start..start + source[start..].find("\n\n").unwrap()];
    let recipe = &recipe[recipe.find('\n').unwrap()..];
    fs::write(
        fixture.0.join("Makefile"),
        format!(
            "TERLAN_RUST_ORCHESTRATOR := $(CURDIR)/owner\npublish-evidence-refresh:{recipe}\npublish-evidence-covered-gates:\n\t@echo entered > entered\n"
        ),
    ).unwrap();
    let owner = fixture.0.join("owner");
    fs::write(
        &owner,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > owner-arguments\nexit 17\n",
    )
    .unwrap();
    fs::set_permissions(owner, fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = Command::new("make");
    command
        .current_dir(&fixture.0)
        .args([
            "--no-print-directory",
            "--warn-undefined-variables",
            "publish-evidence-refresh",
        ])
        .env_remove("MAKEFLAGS")
        .env_remove("MAKEOVERRIDES")
        .env_remove("MFLAGS");
    let output = ProcessControl::new(Duration::from_secs(30))
        .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
        .unwrap();
    assert!(output.outcome.is_err());
    assert!(fs::read_to_string(fixture.0.join("owner-arguments"))
        .unwrap()
        .starts_with("--with-hosted-cargo-coverage -- "));
    assert!(!fixture.0.join("entered").exists());
}

/// Renaming or duplicating a report graph must not conceal its isolated Cargo work.
#[test]
fn refresh_plan_counts_report_owner_and_rejects_missing_or_repeated_work() {
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile")).unwrap();
    let start = source
        .find("\npublish-evidence-refresh-plan-check:\n")
        .unwrap()
        + 1;
    let end = source[start..]
        .find("\npublish: publish-preflight")
        .unwrap()
        + start;
    let producer = "$(MAKE) --no-print-directory -Bn publish-evidence-refresh";
    assert_eq!(source[start..end].matches(producer).count(), 1);
    let recipe = source[start..end].replace(producer, "$(FIXTURE_MAKE)");
    let owner = "terlan-vm run promotion.tvm --script-eval -- prepare-release-reports vm matrix\n";
    let builds = "cargo --locked build -p compiler\ncargo --locked build -p tools\ncargo --locked build --release -p benchmark\ncargo --locked check -p compiler\n";
    let valid = format!("{builds}{owner}");
    for (plan, expected) in [
        (valid.clone(), true),
        (builds.to_owned(), false),
        (format!("{valid}{owner}"), false),
        (format!("{valid}cargo --locked build -p compiler\n"), false),
        (format!("{valid}cargo --locked build -p extra1\ncargo --locked build -p extra2\ncargo --locked build -p extra3\n"), false),
        (format!("{valid}run_exact_cargo_test a\nrun_exact_cargo_test b\nrun_exact_cargo_test c\n"), false),
        (format!("{valid}terlan-vm run scripts_TvmAotPlatformMatrix.tvm --script-eval -- tsan-self-test\n"), false),
    ] {
        let root = std::env::temp_dir().join(format!(
            "terlan-refresh-plan-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let fixture = Fixture(root);
        fs::write(fixture.0.join("Makefile"), &recipe).unwrap();
        let producer = fixture.0.join("plan-producer");
        fs::write(&producer, "#!/bin/sh\nprintf '%s\\n' \"$TERLAN_REFRESH_PLAN\"\n").unwrap();
        fs::set_permissions(&producer, fs::Permissions::from_mode(0o700)).unwrap();
        let mut command = Command::new("make");
        command.current_dir(&fixture.0)
            .args(["--no-print-directory", "publish-evidence-refresh-plan-check", "SHELL=/bin/bash"])
            .env_remove("MAKEFLAGS")
            .env_remove("MAKEOVERRIDES")
            .env_remove("MFLAGS")
            .env("FIXTURE_MAKE", producer)
            .env("TERLAN_REFRESH_PLAN", &plan);
        let output = ProcessControl::new(Duration::from_secs(30))
            .capture_stdout_result(&mut command, 128 * 1024, |_| Ok(()))
            .unwrap();
        assert_eq!(output.outcome.is_ok(), expected, "{plan}");
        if expected {
            assert!(String::from_utf8(output.stdout).unwrap().contains("cargo=4 exact-isolated=1 duplicate-builds=0"));
        }
    }
}
