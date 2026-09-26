use super::*;
use crate::execution_environment::ExecutionEnvironment;
use crate::test_orchestrator_test::temporary_fixture;
use std::path::Path;

#[test]
fn sysroot_output_requires_one_absolute_path() {
    let fixture = temporary_fixture("selected-sysroot-output");
    assert_eq!(
        sysroot_path(format!("{}\r\n", fixture.0.display()).as_bytes()).unwrap(),
        fixture.0
    );
    for output in [
        b"".as_slice(),
        b"relative\n",
        b"/one\n/two\n",
        b"\xff",
        b"/one\0",
    ] {
        assert!(sysroot_path(output).is_err());
    }
}

#[cfg(unix)]
fn executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn wrappers_select_distinct_compilers_and_shared_sysroot_has_one_identity_owner() {
    let fixture = temporary_fixture("selected-compiler-wrappers");
    let root = fixture.0.join("sysroot");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::write(root.join("lib/component"), "before").unwrap();
    let compiler = fixture.0.join("compiler");
    executable(&compiler, "#!/bin/sh\nprintf 'compiler:%s:%s\\n' \"$CONTEXT\" \"$*\" >> \"$TRACE\"\nif test \"$1\" = -vV; then printf 'release: 1.96.0\\nhost: fixture\\n'; else printf '%s\\n' \"$SYSROOT\"; fi\n");
    let outer = fixture.0.join("outer");
    executable(
        &outer,
        "#!/bin/sh\nprintf 'outer:%s\\n' \"$1\" >> \"$TRACE\"\nexec \"$@\"\n",
    );
    let workspace = fixture.0.join("workspace");
    executable(
        &workspace,
        "#!/bin/sh\nexport CONTEXT=workspace\nexec \"$@\"\n",
    );
    let environment = ExecutionEnvironment::from_entries(
        [
            ("PATH".into(), fixture.0.clone().into_os_string()),
            ("SYSROOT".into(), root.clone().into_os_string()),
            ("TRACE".into(), fixture.0.join("trace").into_os_string()),
            ("CONTEXT".into(), "dependency".into()),
        ],
        &fixture.0,
    )
    .unwrap();
    let command = environment.test_command(Path::new("observation"));
    let invocations = vec![
        CompilerInvocation::new(
            true,
            vec![outer.clone(), "workspace".into(), "compiler".into()],
            &command,
        ),
        CompilerInvocation::new(false, vec![outer, "compiler".into()], &command),
    ];
    let control = ProcessControl::new(Duration::from_secs(5));
    let report = fixture.0.join("report.json");
    let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
    let mut selected = SelectedCompiler::admit(invocations, None, &mut ledger, control).unwrap();
    assert_eq!(selected.json()["sysroots"].as_array().unwrap().len(), 1);
    assert_eq!(selected.json()["queries"].as_array().unwrap().len(), 2);
    assert_eq!(fs::read_to_string(fixture.0.join("trace")).unwrap(), "outer:workspace\ncompiler:workspace:-vV\nouter:workspace\ncompiler:workspace:--print sysroot\nouter:compiler\ncompiler:dependency:-vV\nouter:compiler\ncompiler:dependency:--print sysroot\n");
    selected.verify(None, control).unwrap();
    assert!(selected.verified());
    assert!(selected.verify(None, control).is_err());
    ledger.finish().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
    assert_eq!(report["direct_process_launch_count"], 4);
    assert_eq!(report["direct_cargo_launch_count"], 0);
}

#[cfg(unix)]
#[test]
fn selected_compiler_rejects_wrong_version_changed_sysroot_timeout_and_unverified_reuse() {
    for mode in [
        "wrong-version",
        "changed",
        "missing-owner",
        "reused",
        "timeout",
    ] {
        let fixture = temporary_fixture("selected-compiler-rejection");
        let root = fixture.0.join("sysroot");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir(root.join("lib")).unwrap();
        fs::write(root.join("lib/component"), "before").unwrap();
        let compiler = fixture.0.join("compiler");
        let release = if mode == "wrong-version" {
            "0.0.0"
        } else {
            "1.96.0"
        };
        executable(&compiler, &format!("#!/bin/sh\nif test \"$MODE\" = timeout; then exec /bin/sleep 30; fi\nif test \"$1\" = -vV; then printf 'release: {release}\\nhost: fixture\\n'; else printf '%s\\n' \"$SYSROOT\"; fi\n"));
        let environment = ExecutionEnvironment::from_entries(
            [
                ("MODE".into(), mode.into()),
                ("SYSROOT".into(), root.clone().into_os_string()),
            ],
            &fixture.0,
        )
        .unwrap();
        let invocation = CompilerInvocation::new(
            true,
            vec![compiler],
            &environment.test_command(Path::new("observation")),
        );
        let control = ProcessControl::new(if mode == "timeout" {
            Duration::from_millis(30)
        } else {
            Duration::from_secs(5)
        });
        let report = fixture.0.join("report.json");
        let mut ledger = LaunchLedger::new(&report, 1, Duration::from_secs(5)).unwrap();
        let reused = if matches!(mode, "missing-owner" | "reused") {
            Some(ToolTree::capture(&root, control).unwrap())
        } else {
            None
        };
        let admitted =
            SelectedCompiler::admit(vec![invocation], reused.as_ref(), &mut ledger, control);
        if matches!(mode, "wrong-version" | "timeout") {
            assert!(admitted.is_err(), "{mode}");
            assert!(ledger.finish().is_err());
        } else {
            let mut admitted = admitted.unwrap();
            if mode == "changed" {
                fs::write(root.join("lib/component"), "change").unwrap();
            }
            assert_eq!(
                admitted
                    .verify(
                        if mode == "reused" {
                            reused.as_ref()
                        } else {
                            None
                        },
                        control
                    )
                    .is_ok(),
                mode == "reused"
            );
            assert_eq!(admitted.verified(), mode == "reused");
            if mode == "reused" {
                assert_eq!(
                    admitted.json()["sysroots"][0]["identity_owner"],
                    "rustup-toolchain"
                );
            }
        }
    }
}
