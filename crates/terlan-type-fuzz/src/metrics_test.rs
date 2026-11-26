use super::*;

fn case_manifest(family: &str, oracle: &str) -> String {
    format!(
        concat!(
            "schema = \"terlan.type-fuzz-case.v1\"\n",
            "generator = \"terlan.type-fuzz-generator.v6\"\n",
            "compiler_version = \"0.0.10\"\ncompiler_revision = \"revision\"\n",
            "case_id = \"case\"\nseed = 7\ncase_index = 0\nmax_depth = 4\n",
            "family = \"{}\"\nexpected_type = \"Int\"\noracle = \"{}\"\n",
            "case_timeout_ms = 1000\nrss_limit_bytes = 1024\ncpu_limit_ms = 1000\n"
        ),
        family, oracle
    )
}

#[test]
fn aggregate_records_every_required_dimension() {
    let root = tempfile::tempdir().expect("metrics root");
    let run = root.path().join("run");
    let case = run.join("case");
    fs::create_dir_all(run.join("planning")).expect("planning root");
    fs::create_dir(&case).expect("case root");
    fs::write(
        case.join("case.toml"),
        case_manifest("case_product_join", "native"),
    )
    .expect("manifest");
    fs::write(
        case.join("observed.json"),
        r#"{"outcome":"failed","stderr":"error[native_ir.test]: rejected"}"#,
    )
    .expect("observation");
    fs::write(
        case.join("resources.json"),
        r#"{"wall_elapsed_ms":12,"peak_rss_bytes":4096}"#,
    )
    .expect("resources");
    fs::write(case.join("timings.json"), r#"{"events":["one","two"]}"#).expect("timings");
    fs::write(
        case.join("reduction.json"),
        r#"{"failure":{"diagnostic":"error[native_ir.test]"}}"#,
    )
    .expect("reduction");
    fs::write(
        run.join("planning/comparison.json"),
        r#"{"compiler":{"native_image_cache_hits":3,"native_image_cache_misses":1,"cache_miss_reasons":{"source_changed":1}},"nested":[{"native_image_cache_hits":2}]}"#,
    )
    .expect("planning");
    let runs = [RunInput {
        label: "native-s7".into(),
        root: run,
    }];
    publish(root.path(), "candidate", "revision", "failed", &runs).expect("metrics");
    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.path().join("metrics.json")).expect("metrics file"),
    )
    .expect("metrics JSON");
    assert_eq!(report["cases"], 1);
    assert_eq!(report["seeds"], json!([7]));
    assert_eq!(report["features"]["destructure"], 1);
    assert!(report["feature_pairs"]["ordered"]
        .as_array()
        .is_some_and(|pairs| pairs.contains(&json!("case->product"))));
    assert_eq!(report["oracles"]["native"], 1);
    assert_eq!(report["failures"]["error[native_ir.test]"], 1);
    assert_eq!(report["timing"]["events"], 2);
    assert_eq!(report["rss"]["peak_bytes"], 4096);
    assert_eq!(report["cache"]["hits"], 5);
    assert_eq!(report["cache"]["misses"], 1);
    assert_eq!(report["reductions"]["count"], 1);
}

#[test]
fn absent_optional_artifacts_and_unclassified_failures_are_stable() {
    let root = tempfile::tempdir().expect("metrics root");
    let run = root.path().join("run");
    let passed = run.join("passed");
    let failed = run.join("failed");
    fs::create_dir_all(&passed).expect("passed root");
    fs::create_dir(&failed).expect("failed root");
    fs::write(
        passed.join("case.toml"),
        case_manifest("scalar_int", "check"),
    )
    .expect("manifest");
    fs::write(passed.join("observed.json"), r#"{"outcome":"passed"}"#).expect("passed observation");
    fs::write(
        failed.join("case.toml"),
        case_manifest("scalar_unit", "none"),
    )
    .expect("manifest");
    fs::write(failed.join("observed.json"), r#"{"outcome":"failed"}"#).expect("failed observation");
    let runs = [RunInput {
        label: "check-s7".into(),
        root: run,
    }];
    publish(root.path(), "smoke", "revision", "passed", &runs).expect("metrics");
    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.path().join("metrics.json")).expect("metrics file"),
    )
    .expect("metrics JSON");
    assert_eq!(report["cases"], 2);
    assert_eq!(report["outcomes"]["passed"], 1);
    assert_eq!(report["failures"]["unclassified"], 1);
    assert_eq!(report["timing"]["events"], 0);
}

#[test]
fn malformed_inputs_and_publication_failures_are_attributed() {
    let root = tempfile::tempdir().expect("metrics root");
    let missing = [RunInput {
        label: "missing".into(),
        root: root.path().join("missing"),
    }];
    assert!(
        publish(root.path(), "smoke", "revision", "failed", &missing)
            .unwrap_err()
            .contains("read fuzz metric run")
    );

    let run = root.path().join("run");
    let case = run.join("case");
    fs::create_dir_all(&case).expect("case root");
    fs::write(case.join("case.toml"), "not toml").expect("invalid manifest");
    let runs = [RunInput {
        label: "invalid".into(),
        root: run.clone(),
    }];
    assert!(publish(root.path(), "smoke", "revision", "failed", &runs)
        .unwrap_err()
        .contains("invalid fuzz metric case"));

    fs::write(
        case.join("case.toml"),
        case_manifest("scalar_bool", "check"),
    )
    .expect("manifest");
    fs::write(case.join("observed.json"), "not json").expect("invalid observation");
    assert!(publish(root.path(), "smoke", "revision", "failed", &runs)
        .unwrap_err()
        .contains("invalid fuzz metric artifact"));
    fs::remove_file(case.join("observed.json")).expect("remove observation");
    fs::create_dir(root.path().join("metrics.json")).expect("metrics blocker");
    assert!(publish(root.path(), "smoke", "revision", "passed", &runs)
        .unwrap_err()
        .contains("publish fuzz artifact"));
}

#[test]
fn collector_edge_cases_are_stable_and_attributed() {
    let mut aggregate = Aggregate::default();
    assert!(metric_entry_path(Err(std::io::Error::other("entry")))
        .unwrap_err()
        .contains("read fuzz metric entry"));
    let run = RunInput {
        label: "broken".into(),
        root: PathBuf::from("missing"),
    };
    let mut entries = vec![Err(std::io::Error::other("entry"))].into_iter();
    assert!(collect_run_entries(&run, &mut aggregate, &mut entries)
        .unwrap_err()
        .contains("read fuzz metric entry"));
    assert!(collect_case(Path::new("missing-case"), &mut aggregate)
        .unwrap_err()
        .contains("read fuzz metric case"));
    assert!(collect_features("missing-family", &mut aggregate)
        .unwrap_err()
        .contains("unknown fuzz family"));
    assert_eq!(unordered_pair("z", "a"), ("a", "z"));
    assert_eq!(unordered_pair("a", "z"), ("a", "z"));

    collect_cache(&json!(null), &mut aggregate);
    collect_cache(
        &json!({
            "native_image_cache_hits": "invalid",
            "native_image_cache_misses": null,
            "cache_miss_reasons": {"unknown": "invalid"},
            "child": {"cache_miss_reasons": null}
        }),
        &mut aggregate,
    );
    assert_eq!(aggregate.cache_hits, 0);
    assert_eq!(aggregate.cache_misses, 0);
    assert_eq!(aggregate.cache_miss_reasons["unknown"], 0);
    assert_eq!(diagnostic_class("plain failure"), None);
    assert_eq!(diagnostic_class("error[unfinished"), None);
    assert_eq!(oracle_name(Oracle::JavaScript), "javascript");

    let read = attributed_read(
        Path::new("artifact.json"),
        Err(std::io::Error::other("rejected")),
    )
    .expect_err("read failure");
    assert!(read.contains("read fuzz metric artifact"));
    assert!(parse_json_result(
        Path::new("artifact.json"),
        Err(std::io::Error::other("rejected")),
    )
    .unwrap_err()
    .contains("read fuzz metric artifact"));
}

fn publication_error(root: &Path, artifact: &str, contents: &str) -> String {
    let run = root.join(format!("run-{artifact}"));
    let case = run.join("case");
    fs::create_dir_all(&case).expect("case root");
    fs::write(case.join("case.toml"), case_manifest("scalar_int", "check")).expect("manifest");
    fs::write(case.join(artifact), contents).expect("invalid artifact");
    let runs = [RunInput {
        label: artifact.into(),
        root: run,
    }];
    publish(root, "smoke", "revision", "failed", &runs).unwrap_err()
}

#[test]
fn every_optional_artifact_read_failure_is_attributed() {
    let root = tempfile::tempdir().expect("metrics root");
    for artifact in [
        "observed.json",
        "resources.json",
        "timings.json",
        "reduction.json",
    ] {
        assert!(publication_error(root.path(), artifact, "not json")
            .contains("invalid fuzz metric artifact"));
    }

    let run = root.path().join("run-planning");
    fs::create_dir_all(run.join("planning")).expect("planning root");
    fs::write(run.join("planning/comparison.json"), "not json").expect("invalid planning");
    let runs = [RunInput {
        label: "planning".into(),
        root: run,
    }];
    assert!(publish(root.path(), "smoke", "revision", "failed", &runs)
        .unwrap_err()
        .contains("invalid fuzz metric artifact"));

    let family_run = root.path().join("run-family");
    let case = family_run.join("case");
    fs::create_dir_all(&case).expect("case root");
    fs::write(
        case.join("case.toml"),
        case_manifest("missing_family", "check"),
    )
    .expect("manifest");
    let runs = [RunInput {
        label: "family".into(),
        root: family_run,
    }];
    assert!(publish(root.path(), "smoke", "revision", "failed", &runs)
        .unwrap_err()
        .contains("unknown fuzz family"));
}

#[test]
fn optional_artifacts_accept_missing_fields() {
    let root = tempfile::tempdir().expect("metrics root");
    let run = root.path().join("run");
    let case = run.join("case");
    fs::create_dir_all(&case).expect("case root");
    fs::write(
        case.join("case.toml"),
        case_manifest("scalar_string", "java_script"),
    )
    .expect("manifest");
    fs::write(case.join("resources.json"), r#"{}"#).expect("resources");
    fs::write(case.join("timings.json"), r#"{"events":null}"#).expect("timings");
    fs::write(case.join("reduction.json"), r#"{}"#).expect("reduction");
    let runs = [RunInput {
        label: "javascript-s7".into(),
        root: run,
    }];
    publish(root.path(), "candidate", "revision", "passed", &runs).expect("metrics");
    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.path().join("metrics.json")).expect("metrics file"),
    )
    .expect("metrics JSON");
    assert_eq!(report["oracles"]["javascript"], 1);
    assert_eq!(report["timing"]["events"], 0);
    assert_eq!(report["rss"]["peak_bytes"], 0);
    assert_eq!(report["reductions"]["count"], 1);
}
