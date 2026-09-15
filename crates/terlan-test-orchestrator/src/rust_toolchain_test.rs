use super::*;
use crate::test_orchestrator_test::temporary_fixture;
use std::time::Duration;

#[test]
fn selected_release_must_match_one_pinned_channel() {
    let pin = "[toolchain]\nchannel = '1.96.0'\n";
    assert_eq!(
        checked_version_with_pin(b"rustc 1.96.0\nrelease: 1.96.0\nhost: fixture\n", pin).unwrap(),
        "1.96.0"
    );
    for output in [
        b"release: 1.95.0\n".as_slice(),
        b"release: 1.96.0\nrelease: 1.96.0\n",
        b"rustc 1.96.0\n",
        b"\xff",
        b"release: 1.96.0\n",
        b"release: 1.96.0\nhost: first\nhost: second\n",
        b"release: 1.96.0\nhost: \n",
    ] {
        assert!(checked_version_with_pin(output, pin).is_err());
    }
    assert!(checked_version_with_pin(b"release: 1.96.0\n", "malformed").is_err());
    assert!(checked_version_with_pin(b"release: 1.96.0\n", "[toolchain]\n").is_err());
}

#[test]
fn resolved_tools_require_one_absolute_path_and_shared_installation() {
    let fixture = temporary_fixture("rust-toolchain-paths");
    let bin = fixture.0.join("bin");
    fs::create_dir(&bin).unwrap();
    let cargo = bin.join(format!("cargo{}", std::env::consts::EXE_SUFFIX));
    let rustc = bin.join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    fs::write(&cargo, "cargo").unwrap();
    fs::write(&rustc, "rustc").unwrap();
    assert_eq!(
        resolved_path(format!("{}\r\n", cargo.display()).as_bytes()).unwrap(),
        cargo
    );
    for output in [b"relative\n".as_slice(), b"/one\n/two\n", b"", b"\xff"] {
        assert!(resolved_path(output).is_err());
    }
    assert_eq!(installation(&cargo, &rustc).unwrap(), fixture.0);
    assert!(installation(&cargo, &fixture.0.join("other/rustc")).is_err());
    assert!(installation(&cargo, &bin.join("wrapper")).is_err());
}

#[cfg(unix)]
#[test]
fn rustup_custom_toolchain_alias_is_preserved_and_retargeting_fails_closeout() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = temporary_fixture("rust-toolchain-alias");
    let first = fixture.0.join("first");
    let second = fixture.0.join("second");
    for root in [&first, &second] {
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir(root.join("lib")).unwrap();
        for tool in ["cargo", "rustc"] {
            let executable = root.join("bin").join(tool);
            fs::write(&executable, "#!/bin/sh\nprintf 'release: 1.96.0\\n'\n").unwrap();
            fs::set_permissions(executable, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let alias = fixture.0.join("custom");
    symlink(&first, &alias).unwrap();
    let rustup = fixture.0.join("rustup");
    fs::write(
        &rustup,
        "#!/bin/sh\nprintf '%s/bin/%s\\n' \"$FIXTURE_ROOT\" \"$2\"\n",
    )
    .unwrap();
    fs::set_permissions(&rustup, fs::Permissions::from_mode(0o755)).unwrap();
    let environment = ExecutionEnvironment::from_entries(
        [("FIXTURE_ROOT".into(), alias.clone().into_os_string())],
        &fixture.0,
    )
    .unwrap();
    let control = ProcessControl::new(Duration::from_secs(5));
    let executables = ExecutableBinding::capture(
        &[("rustup", rustup), ("cargo", first.join("bin/cargo"))],
        control,
    )
    .unwrap();
    let mut ledger =
        LaunchLedger::new(&fixture.0.join("report.json"), 1, Duration::from_secs(5)).unwrap();
    let mut tools = RustToolchain::admit(&environment, &executables, &mut ledger, control).unwrap();
    assert_eq!(tools.root(), Some(alias.as_path()));
    assert_eq!(
        tools.json()["native_tools"]["before"][0]["path"],
        alias.join("bin/cargo").to_str().unwrap()
    );
    fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    assert!(tools.verify(control).is_err());
    assert!(!tools.verified());
}

#[cfg(unix)]
#[test]
fn observed_resolution_and_toolchain_closeout_reject_mutation() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = temporary_fixture("rust-toolchain-observed");
    let root = fixture.0.join("installation");
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir(root.join("lib")).unwrap();
    let rustup = fixture.0.join("rustup");
    let cargo = root.join("bin/cargo");
    let rustc = root.join("bin/rustc");
    fs::write(&rustup, "#!/bin/sh\ntest \"$RUSTUP_AUTO_INSTALL\" = 0 || exit 9\nif test \"$1\" = show; then printf '%s (default)\\n' \"$FIXTURE_ROOT\"; else printf '%s/bin/%s\\n' \"$FIXTURE_ROOT\" \"$2\"; fi\n").unwrap();
    fs::write(&cargo, "#!/bin/sh\nexit 0\n").unwrap();
    fs::write(
        &rustc,
        "#!/bin/sh\nprintf 'release: %s\\n' \"$FIXTURE_RELEASE\"\n",
    )
    .unwrap();
    for path in [&rustup, &cargo, &rustc] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let control = ProcessControl::new(Duration::from_secs(5));
    let hardlink = fixture.0.join("cargo");
    fs::hard_link(&rustup, &hardlink).unwrap();
    for mode in [
        "matching",
        "hardlink",
        "changed",
        "missing-closeout",
        "late",
    ] {
        let expected_pass = matches!(mode, "matching" | "hardlink");
        fs::write(root.join("lib/component"), "before").unwrap();
        let report = fixture.0.join(format!("{mode}.json"));
        let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
        let environment = ExecutionEnvironment::from_entries(
            [
                ("FIXTURE_ROOT".into(), root.clone().into_os_string()),
                ("FIXTURE_RELEASE".into(), "1.96.0".into()),
            ],
            &fixture.0,
        )
        .unwrap();
        let executables = ExecutableBinding::capture(
            &[
                ("rustup", rustup.clone()),
                (
                    "cargo",
                    if mode == "hardlink" {
                        hardlink.clone()
                    } else {
                        cargo.clone()
                    },
                ),
            ],
            control,
        )
        .unwrap();
        let admitted = RustToolchain::admit(&environment, &executables, &mut ledger, control);
        if mode == "late" {
            ledger
                .execute(
                    "late fixture",
                    ValidationTier::FastUnit,
                    "fixture",
                    |observe| {
                        let mut command = std::process::Command::new("/bin/true");
                        control.run(&mut command, observe).map_err(process_failure)
                    },
                )
                .unwrap();
            assert!(ledger.bind_toolchain(admitted.unwrap()).is_err());
        } else {
            ledger.bind_toolchain(admitted.unwrap()).unwrap();
            if mode == "changed" {
                fs::write(root.join("lib/component"), "change").unwrap();
            }
            if mode != "missing-closeout" {
                assert_eq!(ledger.verify_toolchain(control).is_ok(), expected_pass);
            }
        }
        assert_eq!(ledger.finish().is_ok(), expected_pass);
        let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
        assert_eq!(
            report["direct_process_launch_count"],
            if matches!(mode, "late" | "hardlink") {
                3
            } else {
                2
            }
        );
        assert_eq!(report["direct_cargo_launch_count"], 0);
        assert_eq!(
            report["decision"],
            if expected_pass { "pass" } else { "fail" }
        );
        if mode == "changed" {
            assert_ne!(
                report["rust_toolchain_binding"]["before"],
                report["rust_toolchain_binding"]["after"]
            );
        }
    }
}
