use super::*;

#[test]
fn completion_requires_exact_admitted_target_owner_and_harness_counts() {
    let target = Target {
        package: "example".into(),
        crate_name: "example".into(),
        manifest: "/workspace/Cargo.toml".into(),
        source: "/workspace/lib.rs".into(),
        edition: "2024".into(),
    };
    let started = json!({"role":"rustdoc-observer", "pid":10});
    let run = json!({"role":"rustdoc", "pid":11});
    let execution = crate::doctest_output::inspect(b"running 1 test\ntest lib.rs - (line 1) ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n").unwrap();
    let completion = json!({"schema":"terlan.rustdoc-completion.v1", "context_identity":"context", "target":target,
        "launches":[started,run], "decision":"pass", "test_execution":execution.json()});
    assert_eq!(
        admit_completion(&completion, &target, "context", &started, &run).unwrap(),
        1
    );
    for (pointer, value) in [
        ("/schema", json!("other")),
        ("/context_identity", json!("stale")),
        ("/target/edition", json!("2021")),
        ("/launches", json!([])),
        ("/decision", json!("fail")),
        ("/test_execution/passed", json!(2)),
        ("/test_execution/harnesses", json!([])),
        ("/test_execution/independent_inventory", json!(true)),
        ("/test_execution/scope", json!("summary")),
    ] {
        let mut changed = completion.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            admit_completion(&changed, &target, "context", &started, &run).is_err(),
            "{pointer}"
        );
    }
    let mut changed = completion.clone();
    let invalid = json!({"role":"rustdoc", "pid":0});
    changed["launches"] = json!([started, invalid]);
    assert!(admit_completion(&changed, &target, "context", &started, &invalid).is_err());
    let reused = json!({"role":"rustdoc", "pid":10});
    changed["launches"] = json!([started, reused]);
    assert!(admit_completion(&changed, &target, "context", &started, &reused).is_err());
}

#[test]
fn observer_mode_is_selected_only_by_the_private_executable_name() {
    assert!(is_observer(
        Path::new("/private").join(observer_name()).as_os_str()
    ));
    for name in [
        "terlan-test-orchestrator",
        "rustdoc",
        "terlan-rustdoc-observer-extra",
    ] {
        assert!(!is_observer(std::ffi::OsStr::new(name)));
    }
}

#[cfg(unix)]
#[test]
fn successful_cargo_exit_cannot_replace_a_missing_rustdoc_completion() {
    let fixture = crate::test_orchestrator_test::temporary_fixture("missing-rustdoc-completion");
    let environment = ExecutionEnvironment::from_entries(std::env::vars_os(), &fixture.0).unwrap();
    let no_work =
        crate::executable_binding::resolve_program("true", &environment.value("PATH").unwrap())
            .unwrap();
    let driver = std::env::current_exe().unwrap();
    let inputs = Inputs {
        rustdoc: no_work.clone(),
        settings: Default::default(),
        targets: vec![Target {
            package: "example".into(),
            crate_name: "example".into(),
            manifest: fixture.0.join("Cargo.toml"),
            source: fixture.0.join("lib.rs"),
            edition: "2024".into(),
        }],
    };
    let mut launches = Vec::new();
    let mut partial = None;
    let error = run(
        &crate::phase_plan::workspace_doctest_phase(),
        (&no_work, &driver, &inputs),
        &environment,
        1,
        ProcessControl::new(std::time::Duration::from_secs(10)),
        &mut |pid| {
            launches.push(pid);
            Ok(())
        },
        &mut partial,
    )
    .unwrap_err();
    assert!(error.detail.contains("No such file"), "{}", error.detail);
    assert_eq!(launches.len(), 1);
    let partial = partial.unwrap().json();
    assert_eq!(partial["decision"], "fail");
    assert_eq!(partial["complete"], false);
    assert_eq!(partial["nested_process_launch_count"], 0);
}
