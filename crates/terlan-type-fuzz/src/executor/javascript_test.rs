use std::fs;

use super::*;

fn limits() -> Limits {
    Limits {
        wall_ms: 1_000,
        rss_bytes: 1024 * 1024 * 1024,
        cpu_ms: 1_000,
    }
}

fn result(outcome: &'static str) -> OracleResult {
    let mut result = oracle::infrastructure_result(
        Oracle::JavaScript,
        "fixture",
        limits().wall_ms,
        limits().rss_bytes,
        limits().cpu_ms,
    );
    result.outcome = outcome;
    result
}

fn source(module: &str) -> String {
    format!("module {module}.\n\n@test\npub sample_matches_expected(): Bool -> true.\n")
}

#[test]
fn module_paths_are_canonical_and_reject_malformed_declarations() {
    assert_eq!(
        module_relative_path(&source("type_fuzz.Case000001")),
        Ok(PathBuf::from("type_fuzz/Case000001"))
    );
    for malformed in [
        "",
        "module missing_terminator",
        "module bad-name.Case.",
        "module valid..Missing.",
        "module 7invalid.Case.",
    ] {
        assert!(module_relative_path(malformed).is_err(), "{malformed}");
    }
}

#[test]
fn workspace_staging_is_exact_and_rejects_bad_inputs() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("first.terl"), source("probe.First")).expect("first");
    fs::write(root.path().join("second.terl"), source("probe.Second")).expect("second");
    let workspace = root.path().join("workspace");
    let modules = prepare_workspace(
        root.path(),
        &[PathBuf::from("first.terl"), PathBuf::from("second.terl")],
        &workspace,
    )
    .expect("stage");
    assert_eq!(
        modules,
        [
            PathBuf::from("probe/First.js"),
            PathBuf::from("probe/Second.js")
        ]
    );
    assert_eq!(
        fs::read_to_string(workspace.join("src/probe/First.terl")).expect("staged first"),
        source("probe.First")
    );

    fs::write(root.path().join("duplicate.terl"), source("probe.First")).expect("duplicate");
    assert!(prepare_workspace(
        root.path(),
        &[PathBuf::from("first.terl"), PathBuf::from("duplicate.terl")],
        &workspace,
    )
    .unwrap_err()
    .contains("duplicate"));
    assert!(
        prepare_workspace(root.path(), &[PathBuf::from("missing.terl")], &workspace,)
            .unwrap_err()
            .contains("read generated")
    );
    fs::write(root.path().join("bad.terl"), "not a module").expect("bad source");
    assert!(
        prepare_workspace(root.path(), &[PathBuf::from("bad.terl")], &workspace,)
            .unwrap_err()
            .contains("canonical module")
    );
    let oversized_module = "a".repeat(300);
    fs::write(
        root.path().join("oversized.terl"),
        source(&oversized_module),
    )
    .expect("oversized module source");
    assert!(
        prepare_workspace(root.path(), &[PathBuf::from("oversized.terl")], &workspace,)
            .unwrap_err()
            .contains("write JavaScript module stage")
    );
}

#[cfg(unix)]
#[test]
fn workspace_staging_attributes_every_filesystem_boundary() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("Case.terl"), source("probe.Case")).expect("source");

    let reset = root.path().join("reset");
    fs::create_dir(&reset).expect("reset root");
    fs::write(reset.join("child"), "child").expect("reset child");
    fs::set_permissions(&reset, fs::Permissions::from_mode(0o000)).expect("lock reset root");
    let error = prepare_workspace(root.path(), &[PathBuf::from("Case.terl")], &reset)
        .expect_err("locked workspace must not reset");
    fs::set_permissions(&reset, fs::Permissions::from_mode(0o700)).expect("unlock reset root");
    assert!(error.contains("reset JavaScript oracle"));

    let blocked_root = root.path().join("blocked-root");
    fs::create_dir(&blocked_root).expect("blocked root");
    fs::set_permissions(&blocked_root, fs::Permissions::from_mode(0o500))
        .expect("lock blocked root");
    let error = prepare_workspace(
        root.path(),
        &[PathBuf::from("Case.terl")],
        &blocked_root.join("workspace"),
    )
    .expect_err("locked parent must reject stage");
    fs::set_permissions(&blocked_root, fs::Permissions::from_mode(0o700))
        .expect("unlock blocked root");
    assert!(error.contains("create JavaScript stage"));

    let stage = root.path().join("stage");
    fs::create_dir(&stage).expect("stage");
    fs::write(stage.join("probe"), "blocking file").expect("blocking parent");
    assert!(stage_source(&stage, Path::new("probe/Case"), "source")
        .unwrap_err()
        .contains("create JavaScript module stage"));
    fs::create_dir(stage.join("Case.terl")).expect("blocking destination");
    assert!(stage_source(&stage, Path::new("Case"), "source")
        .unwrap_err()
        .contains("write JavaScript module stage"));
}

#[test]
fn harness_contains_one_strict_check_per_module() {
    let root = tempfile::tempdir().expect("root");
    write_harness(
        root.path(),
        &[
            PathBuf::from("probe/First.js"),
            PathBuf::from("probe/Second.js"),
        ],
    )
    .expect("harness");
    assert_eq!(
        fs::read_to_string(root.path().join("package.json")).expect("package"),
        "{\"type\":\"module\"}\n"
    );
    let harness = fs::read_to_string(root.path().join("type-fuzz-harness.mjs")).expect("harness");
    assert!(harness.contains("./modules/probe/First.js"));
    assert!(harness.contains("case0.sample_matches_expected() !== true"));
    assert!(harness.contains("./modules/probe/Second.js"));
    assert!(harness.contains("case1.sample_matches_expected() !== true"));

    let blocked = root.path().join("blocked");
    fs::write(&blocked, "file").expect("block root");
    assert!(write_harness(&blocked, &[])
        .unwrap_err()
        .contains("create JavaScript runtime root"));

    let package_blocked = root.path().join("package-blocked");
    fs::create_dir(&package_blocked).expect("package blocked root");
    fs::create_dir(package_blocked.join("package.json")).expect("package blocker");
    assert!(write_harness(&package_blocked, &[])
        .unwrap_err()
        .contains("write JavaScript runtime package"));

    let harness_blocked = root.path().join("harness-blocked");
    fs::create_dir(&harness_blocked).expect("harness blocked root");
    fs::create_dir(harness_blocked.join("type-fuzz-harness.mjs")).expect("harness blocker");
    assert!(write_harness(&harness_blocked, &[])
        .unwrap_err()
        .contains("write JavaScript runtime harness"));
}

#[cfg(unix)]
#[test]
fn harness_rejects_non_utf8_module_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().expect("root");
    let module = PathBuf::from(OsString::from_vec(vec![0xff]));
    assert!(write_harness(root.path(), &[module])
        .unwrap_err()
        .contains("not UTF-8"));
}

#[cfg(unix)]
#[test]
fn build_and_node_phases_are_bounded_and_attributed() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("Case.terl"), source("probe.Case")).expect("source");
    let compiler = root.path().join("compiler");
    fs::write(&compiler, "#!/bin/sh\nprintf 'build-ok'\n").expect("compiler");
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700)).expect("permissions");
    let node = root.path().join("node");
    fs::write(&node, "#!/bin/sh\nprintf 'node-ok'\n").expect("node");
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).expect("permissions");
    let workspace = root.path().join("workspace");
    let result = run_in_workspace(
        &compiler,
        &node,
        root.path(),
        &[PathBuf::from("Case.terl")],
        &workspace,
        limits(),
    );
    assert_eq!(result.outcome, "passed");
    assert_eq!(result.stdout, "build-ok\nnode-ok");
    assert!(workspace.join("src/probe/Case.terl").is_file());
    assert!(workspace.join("out/js/type-fuzz-harness.mjs").is_file());

    fs::write(&compiler, "#!/bin/sh\nexit 7\n").expect("failing compiler");
    let failed = run_in_workspace(
        &compiler,
        &node,
        root.path(),
        &[PathBuf::from("Case.terl")],
        &workspace,
        limits(),
    );
    assert_eq!((failed.outcome, failed.exit_code), ("failed", Some(7)));

    fs::write(&compiler, "#!/bin/sh\nexit 0\n").expect("compiler");
    fs::write(&node, "#!/bin/sh\nexit 8\n").expect("failing node");
    let failed = run_in_workspace(
        &compiler,
        &node,
        root.path(),
        &[PathBuf::from("Case.terl")],
        &workspace,
        limits(),
    );
    assert_eq!((failed.outcome, failed.exit_code), ("failed", Some(8)));

    let missing = run_in_workspace(
        &compiler,
        &node,
        root.path(),
        &[PathBuf::from("Missing.terl")],
        &workspace,
        limits(),
    );
    assert_eq!(missing.outcome, "infrastructure_error");

    fs::write(
        &compiler,
        format!(
            "#!/bin/sh\nmkdir -p '{}'\nprintf blocked > '{}'/js\n",
            workspace.join("out").display(),
            workspace.join("out").display()
        ),
    )
    .expect("harness-blocking compiler");
    let blocked = run_in_workspace(
        &compiler,
        &node,
        root.path(),
        &[PathBuf::from("Case.terl")],
        &workspace,
        limits(),
    );
    assert_eq!(blocked.outcome, "infrastructure_error");
    assert!(blocked.stderr.contains("create JavaScript runtime root"));
}

#[test]
fn workspace_cleanup_handles_missing_files_and_directories() {
    let root = tempfile::tempdir().expect("root");
    let missing = root.path().join("missing");
    remove_workspace(&missing).expect("missing workspace");
    fs::write(&missing, "file").expect("workspace file");
    remove_workspace(&missing).expect("file workspace");
    fs::create_dir(&missing).expect("workspace directory");
    fs::write(missing.join("value"), "value").expect("workspace value");
    remove_workspace(&missing).expect("directory workspace");
    assert!(!missing.exists());
}

#[cfg(unix)]
#[test]
fn cleanup_failures_are_attributed_without_hiding_prior_failures() {
    use std::os::unix::fs::PermissionsExt;

    for outcome in ["passed", "failed"] {
        let root = tempfile::tempdir().expect("root");
        let workspace = root.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");
        fs::write(workspace.join("child"), "child").expect("child");
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o000)).expect("lock workspace");
        let finished = finish(result(outcome), &workspace, limits());
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700))
            .expect("unlock workspace");
        if outcome == "passed" {
            assert_eq!(finished.outcome, "infrastructure_error");
            assert!(finished
                .stderr
                .contains("clean JavaScript oracle workspace"));
        } else {
            assert_eq!(finished.outcome, "failed");
            assert_eq!(finished.stderr, "fixture");
        }
    }
}

#[cfg(unix)]
#[test]
fn cleanup_attributes_metadata_errors() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("root");
    let parent = root.path().join("locked");
    fs::create_dir(&parent).expect("locked parent");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o000)).expect("lock parent");
    let error = remove_workspace(&parent.join("workspace")).expect_err("metadata must fail");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).expect("unlock parent");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}

#[cfg(unix)]
#[test]
fn public_oracle_attributes_build_failure_and_cleans_workspace() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("Case.terl"), source("probe.Case")).expect("source");
    let result = run(
        Path::new("/bin/false"),
        root.path(),
        &[PathBuf::from("Case.terl")],
        1_000,
        1024 * 1024 * 1024,
        1_000,
    );
    assert_eq!(result.outcome, "failed");
    assert!(!root.path().join(WORKSPACE).exists());

    let dispatched = oracle::run_batch(
        Path::new("/bin/true"),
        Oracle::JavaScript,
        root.path(),
        &[PathBuf::from("Missing.terl")],
        1_000,
        1024 * 1024 * 1024,
        1_000,
    );
    assert_eq!(dispatched.outcome, "infrastructure_error");
    assert!(dispatched.stderr.contains("read generated JavaScript case"));
}
