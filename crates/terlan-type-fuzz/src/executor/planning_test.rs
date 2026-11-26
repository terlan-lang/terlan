use super::*;

#[test]
fn disabled_planning_probe_is_inert() {
    let root = tempfile::tempdir().expect("planning root");
    let options = RunOptions {
        oracle: Oracle::Check,
        out_dir: root.path().join("cases"),
        ..RunOptions::default()
    };
    run(&options, Path::new("missing"), 10).expect("inactive probe");
    assert!(!options.out_dir.join("planning").exists());
}

#[cfg(unix)]
#[test]
fn matched_planning_graphs_record_bounded_evidence() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("planning root");
    let compiler = root.path().join("compiler");
    fs::write(&compiler, attributed_compiler_script("true")).expect("compiler fixture");
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700))
        .expect("compiler permissions");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: root.path().join("cases"),
        case_timeout_ms: 1_000,
        total_timeout_ms: 2_000,
        rss_limit_bytes: 1024 * 1024 * 1024,
        cpu_limit_ms: 1_000,
        ..RunOptions::default()
    };
    run(&options, &compiler, 2_000).expect("planning probe");
    let planning = options.out_dir.join("planning");
    let comparison: serde_json::Value = serde_json::from_slice(
        &fs::read(planning.join("comparison.json")).expect("planning comparison"),
    )
    .expect("planning comparison JSON");
    assert_eq!(comparison["schema"], "terlan.type-fuzz-planning.v1");
    assert_eq!(comparison["narrow"]["cold"]["compiler"]["timing_events"], 4);
    assert_eq!(
        comparison["combined_root"]["cold"]["compiler"]["timing_events"],
        4
    );
    assert_eq!(
        comparison["delta"]["cold_broad_minus_narrow"]["compiler_work"]["native_units_built"],
        0
    );
    assert!(planning.join("narrow").join(NARROW_SOURCE).is_file());
    assert!(planning.join("combined").join(COMBINED_FIRST).is_file());
    assert!(planning.join("combined").join(COMBINED_SECOND).is_file());
    assert!(planning.join("broad").join(BROAD_SOURCE).is_file());
    assert!(planning.join("broad/unused-native/terlan.toml").is_file());
}

#[cfg(unix)]
#[test]
fn planning_failures_and_metrics_are_attributed() {
    let root = tempfile::tempdir().expect("planning root");
    let options = RunOptions {
        oracle: Oracle::Native,
        out_dir: root.path().join("cases"),
        ..RunOptions::default()
    };
    assert!(run(&options, Path::new("/bin/false"), 2_000)
        .unwrap_err()
        .contains("narrow cold native planning probe"));
    assert_eq!(signed_delta(Some(9), Some(4)), Some(5));
    assert_eq!(signed_delta(None, Some(4)), None);
    assert_eq!(signed_delta(Some(4), None), None);
    let events = concat!(
        "terlc timing: frontend.parse: +1ms total=1ms\n",
        "terlc native-aot: module-unit.begin.demo functions=1 continuations=0\n",
    );
    assert_eq!(timing_count(events), 2);
    assert_eq!(
        prefix_count(events, "terlc native-aot: module-unit.begin."),
        1
    );
    assert_eq!(signed_count_delta(events, ""), 2);
    assert_eq!(signed_prefix_delta(events, "", "terlc native-aot:"), 1);
}

#[test]
fn every_nested_fixture_write_failure_is_attributed() {
    for blocked in [
        NARROW_SOURCE,
        "unused-native/terlan.toml",
        "unused-native/src/type_fuzz/unused_native/Unused.terl",
    ] {
        let root = tempfile::tempdir().expect("planning root");
        let path = root.path().join(blocked);
        fs::create_dir_all(&path).expect("blocking directory");
        let error = write_fixture(root.path(), blocked != NARROW_SOURCE)
            .expect_err("fixture write must fail");
        assert!(error.contains("write"), "{blocked}: {error}");
    }

    for blocked in ["terlan.toml", COMBINED_FIRST, COMBINED_SECOND] {
        let root = tempfile::tempdir().expect("planning root");
        let path = root.path().join(blocked);
        fs::create_dir_all(&path).expect("blocking directory");
        let error = write_combined_fixture(root.path()).expect_err("fixture write must fail");
        assert!(error.contains("write"), "{blocked}: {error}");
    }

    let root = tempfile::tempdir().expect("planning root");
    let blocked_parent = root.path().join("blocked");
    fs::write(&blocked_parent, "file").expect("blocking file");
    assert!(write(blocked_parent.join("value"), "value")
        .unwrap_err()
        .contains("create planning fixture"));
}

#[cfg(unix)]
#[test]
fn comparison_publication_failure_is_attributed() {
    let root = tempfile::tempdir().expect("planning root");
    fs::create_dir(root.path().join("comparison.json")).expect("blocking comparison");
    assert!(publish(root.path(), &serde_json::json!({}))
        .unwrap_err()
        .contains("write planning comparison"));
}

#[cfg(unix)]
#[test]
fn broad_execution_failure_is_attributed() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("planning root");
    let launches = root.path().join("launches");
    let compiler = root.path().join("compiler");
    fs::write(
        &compiler,
        attributed_compiler_script(&format!(
            "printf 'launch\\n' >> {}\n[ $(wc -l < {}) -lt 5 ]",
            launches.display(),
            launches.display(),
        )),
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
        .contains("broad cold native planning probe"));
}

#[test]
fn fixture_root_failures_are_attributed() {
    let root = tempfile::tempdir().expect("planning root");
    let narrow = root.path().join("narrow");
    let combined = root.path().join("combined");
    let broad = root.path().join("broad");
    fs::write(&narrow, "file").expect("blocking narrow profile");
    assert!(write_fixtures(&narrow, &combined, &broad)
        .unwrap_err()
        .contains("create planning fixture"));

    let root = tempfile::tempdir().expect("planning root");
    let narrow = root.path().join("narrow");
    let combined = root.path().join("combined");
    let broad = root.path().join("broad");
    fs::write(&combined, "file").expect("blocking combined profile");
    assert!(write_fixtures(&narrow, &combined, &broad)
        .unwrap_err()
        .contains("create planning fixture"));

    let root = tempfile::tempdir().expect("planning root");
    let narrow = root.path().join("narrow");
    let combined = root.path().join("combined");
    let broad = root.path().join("broad");
    fs::write(&broad, "file").expect("blocking broad profile");
    assert!(write_fixtures(&narrow, &combined, &broad)
        .unwrap_err()
        .contains("create planning fixture"));

    let root = tempfile::tempdir().expect("planning root");
    fs::create_dir(root.path().join("terlan.toml")).expect("blocking manifest");
    assert!(write_fixture(root.path(), false)
        .unwrap_err()
        .contains("write"));
}
