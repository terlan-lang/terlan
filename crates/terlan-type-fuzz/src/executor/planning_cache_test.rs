use super::*;
use crate::executor::oracle;

#[test]
fn planning_profiles_enforce_the_hard_ceiling() {
    assert_eq!(profiles::planning_wall_limit(u64::MAX), 60_000);
    assert_eq!(profiles::planning_wall_limit(59_999), 59_999);
    assert_eq!(profiles::planning_rss_limit(u64::MAX), 1024 * 1024 * 1024);
    assert_eq!(profiles::planning_rss_limit(1024), 1024);
}

#[test]
fn planning_failures_retain_phase_graph_and_resource_evidence() {
    let root = tempfile::tempdir().expect("planning root");
    let source = root.path().join("CaseTest.terl");
    fs::write(&source, "module planning.CaseTest.").expect("source");
    let mut result = oracle::run(
        Path::new("/bin/false"),
        Oracle::Native,
        &source,
        1_000,
        1024,
        1_000,
    );
    result.stderr = concat!(
        "terlc timing: test.project-complete.finish: +9ms total=9ms ",
        "reachable-declarations=2 native-providers=none ",
        "cache-miss-reason=source_changed\n",
    )
    .into();
    let failure = profiles::ensure_passed("focused cold", &result)
        .expect_err("failed compiler must retain planning evidence");
    assert!(failure.contains("terlan.type-fuzz-planning-failure.v1"));
    assert!(failure.contains("test.project-complete.finish"));
    assert!(failure.contains("reachable_declarations\":2"));
    assert!(failure.contains("source_changed"));
    assert!(failure.contains("rss_limit_bytes\":1024"));
}

#[cfg(unix)]
#[test]
fn every_warm_profile_failure_is_attributed() {
    use std::os::unix::fs::PermissionsExt;

    for (failure, expected) in [
        (2, "narrow warm"),
        (3, "combined cold"),
        (4, "combined warm"),
        (6, "broad warm"),
    ] {
        let root = tempfile::tempdir().expect("planning root");
        let launches = root.path().join("launches");
        let compiler = root.path().join("compiler");
        fs::write(
            &compiler,
            format!(
                "#!/bin/sh\nprintf 'launch\\n' >> {}\n[ $(wc -l < {}) -ne {failure} ]\n",
                launches.display(),
                launches.display(),
            ),
        )
        .expect("compiler fixture");
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
            .expect("compiler permissions");
        let options = RunOptions {
            oracle: Oracle::Native,
            out_dir: root.path().join("cases"),
            ..RunOptions::default()
        };
        assert!(run(&options, &compiler, 2_000)
            .unwrap_err()
            .contains(&format!("{expected} native planning probe")));
    }
}

#[test]
fn planning_root_reset_failures_are_attributed() {
    let root = tempfile::tempdir().expect("planning root");
    let output = root.path().join("cases");
    fs::create_dir(&output).expect("cases root");
    fs::write(output.join("planning"), "not a directory").expect("blocking file");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: output,
        ..RunOptions::default()
    };
    assert!(run(&options, Path::new("missing"), 2_000)
        .unwrap_err()
        .contains("reset planning fixtures"));
}

#[test]
fn planning_fixture_creation_failure_is_attributed_by_run() {
    let root = tempfile::tempdir().expect("planning root");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: root.path().join("x".repeat(300)),
        ..RunOptions::default()
    };
    assert!(run(&options, Path::new("missing"), 2_000)
        .unwrap_err()
        .contains("create planning fixture"));
}

#[test]
fn broad_regressions_are_rejected_by_owned_dimension() {
    let root = tempfile::tempdir().expect("planning root");
    let source = root.path().join("CaseTest.terl");
    fs::write(&source, "module planning.CaseTest.").expect("source");
    let result = || {
        oracle::run(
            Path::new("/bin/true"),
            Oracle::Native,
            &source,
            1_000,
            1024 * 1024 * 1024,
            1_000,
        )
    };

    let narrow = result();
    let mut broad = result();
    broad.stderr = "terlc native-aot: module-unit.begin.extra functions=1 continuations=0".into();
    assert!(profiles::ensure_broad_regression_absent(&broad, &narrow)
        .unwrap_err()
        .contains("compiler work"));

    broad.stderr.clear();
    broad.resources.wall_elapsed_ms = Some(2_000);
    broad.resources.peak_rss_bytes = narrow.resources.peak_rss_bytes;
    assert!(profiles::ensure_broad_regression_absent(&broad, &narrow)
        .unwrap_err()
        .contains("wall tolerance"));

    broad.resources.wall_elapsed_ms = narrow.resources.wall_elapsed_ms;
    broad.resources.peak_rss_bytes = Some(u64::MAX);
    assert!(profiles::ensure_broad_regression_absent(&broad, &narrow)
        .unwrap_err()
        .contains("RSS tolerance"));
    assert!(profiles::within_increase(None, None, 1));
    assert!(!profiles::within_increase(Some(1), None, 1));
}

#[cfg(unix)]
#[test]
fn broad_work_regression_fails_the_complete_probe() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("planning root");
    let launches = root.path().join("launches");
    let compiler = root.path().join("compiler");
    fs::write(
        &compiler,
        format!(
            "#!/bin/sh\nprintf 'launch\\n' >> {}\nif [ $(wc -l < {}) -eq 5 ]; then echo 'terlc native-aot: module-unit.begin.extra functions=1 continuations=0' >&2; fi\n",
            launches.display(),
            launches.display(),
        ),
    )
    .expect("compiler fixture");
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
        .expect("compiler permissions");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: root.path().join("cases"),
        ..RunOptions::default()
    };
    assert!(run(&options, &compiler, 2_000)
        .unwrap_err()
        .contains("added compiler work"));
}
