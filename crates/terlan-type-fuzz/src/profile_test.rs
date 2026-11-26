use super::*;

fn request(profile: FuzzProfile, root: &Path) -> ProfileRequest {
    ProfileRequest {
        profile,
        out_dir: root.to_path_buf(),
        rotation: 0,
        changed_paths: Vec::new(),
    }
}

#[test]
fn named_profiles_are_bounded_and_cover_the_target_matrix() {
    let root = Path::new("profiles");
    let smoke = plan(&request(FuzzProfile::Smoke, root)).expect("smoke plan");
    assert_eq!(smoke.len(), 1);
    assert_eq!(smoke[0].options.oracle, Oracle::Check);
    assert_eq!(smoke[0].options.cases, 22);

    let candidate = plan(&request(FuzzProfile::Candidate, root)).expect("candidate plan");
    assert_eq!(candidate.len(), 3);
    assert_eq!(candidate[0].options.families.len(), 22);
    assert_eq!(candidate[1].options.oracle, Oracle::Native);
    assert_eq!(candidate[2].options.families.len(), 14);

    let mut scheduled_request = request(FuzzProfile::Scheduled, root);
    scheduled_request.rotation = 7;
    let scheduled = plan(&scheduled_request).expect("scheduled plan");
    assert_eq!(scheduled.len(), 6);
    assert!(scheduled.iter().all(|run| run.options.cases >= 28));
    assert_ne!(scheduled[0].options.seed, scheduled[3].options.seed);
    assert!(scheduled.iter().all(|run| {
        run.options.total_timeout_ms == 300_000
            && run.options.rss_limit_bytes == 2 * 1024 * 1024 * 1024
    }));

    let invalid = request(FuzzProfile::Impacted, root);
    assert!(plan(&invalid).unwrap_err().contains("--changed-path"));
}

#[test]
fn impacted_profile_uses_explicit_owned_families() {
    let root = Path::new("profiles");
    let plans = |path: &str| {
        let mut request = request(FuzzProfile::Impacted, root);
        request.changed_paths.push(path.into());
        plan(&request).expect("impacted plan")
    };
    let frontend = plans("crates/terlan/src/compiler/typeck/expression.rs");
    assert_eq!(frontend.len(), 3);
    assert_eq!(frontend[0].options.families.len(), 22);
    assert_eq!(frontend[1].options.families.len(), 14);
    assert_eq!(frontend[2].options.families.len(), 22);

    let javascript = plans("crates/terlan/src/commands/emit_js/direct_ast.rs");
    assert_eq!(javascript.len(), 1);
    assert_eq!(javascript[0].options.oracle, Oracle::JavaScript);
    assert_eq!(javascript[0].options.cases, 14);

    let runtime = plans("crates/terlan/src/runtime/vm.rs");
    assert_eq!(runtime.len(), 1);
    assert_eq!(runtime[0].options.oracle, Oracle::Native);

    let interfaces = plans("crates/terlan/src/commands/build/source_roots.rs");
    assert_eq!(interfaces.len(), 2);
    assert!(interfaces.iter().all(|run| run.options.cases == 3));

    let all = plans("Cargo.lock");
    assert_eq!(all.len(), 3);
    assert!(plans("docs/README.md").is_empty());

    let mut overlapping = request(FuzzProfile::Impacted, root);
    overlapping.changed_paths = vec![
        "Cargo.toml".into(),
        "crates/terlan-type-fuzz/src/lib.rs".into(),
        "std/core/Int.terl".into(),
        "crates/terlan/src/commands/fuzz/mod.rs".into(),
        "crates/terlan/src/formal_pipeline.rs".into(),
        "crates/terlan/src/runtime/native_image.rs".into(),
    ];
    let deduplicated = plan(&overlapping).expect("deduplicated ownership");
    assert_eq!(deduplicated.len(), 3);
    assert_eq!(deduplicated[0].options.families.len(), 22);
}

#[test]
fn successful_result_is_revision_bound_and_reusable() {
    let root = tempfile::tempdir().expect("profile root");
    let mut request = request(FuzzProfile::Impacted, root.path());
    request.changed_paths.push("docs/README.md".into());
    run(&request, Path::new("missing-compiler"), "revision-a").expect("empty impacted run");
    let result = root.path().join("result.json");
    verify(&result, "revision-a").expect("matching revision");
    assert!(verify(&result, "revision-b")
        .unwrap_err()
        .contains("does not match candidate"));

    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(result).expect("result text"))
            .expect("result json");
    assert_eq!(value["status"], "passed");
    assert_eq!(value["runs"], serde_json::json!([]));
}

#[test]
fn result_records_every_planned_backend() {
    let root = tempfile::tempdir().expect("profile root");
    let request = request(FuzzProfile::Candidate, root.path());
    let runs = plan(&request).expect("candidate plan");
    publish_result(&request, "revision", &runs).expect("profile result");

    let value: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join("result.json")).expect("result text"),
    )
    .expect("result json");
    assert_eq!(value["runs"].as_array().map(Vec::len), Some(3));
    assert_eq!(value["runs"][0]["oracle"], "check");
    assert_eq!(value["runs"][1]["oracle"], "native");
    assert_eq!(value["runs"][2]["oracle"], "javascript");
    assert_eq!(oracle_name(Oracle::None), "none");
}

#[test]
fn profile_and_result_failures_are_attributed() {
    let root = tempfile::tempdir().expect("failure root");
    assert!(verify(&root.path().join("missing.json"), "revision")
        .unwrap_err()
        .contains("read fuzz profile result"));
    for (value, expected) in [
        ("not-json", "invalid fuzz profile result"),
        (
            r#"{"schema":"old","compiler_revision":"revision","profile":"smoke","rotation":0,"status":"passed","runs":[]}"#,
            "unsupported fuzz profile result",
        ),
        (
            r#"{"schema":"terlan.type-fuzz-profile-result.v1","compiler_revision":"revision","profile":"smoke","rotation":0,"status":"failed","runs":[]}"#,
            "ended as `failed`",
        ),
    ] {
        let path = root.path().join("candidate.json");
        fs::write(&path, value).expect("candidate result");
        assert!(verify(&path, "revision").unwrap_err().contains(expected));
    }

    let blocked = root.path().join("blocked");
    fs::write(&blocked, "file").expect("blocked output");
    assert!(run(
        &request(FuzzProfile::Smoke, &blocked),
        Path::new("missing"),
        "revision"
    )
    .unwrap_err()
    .contains("create fuzz profile output"));

    let publication = root.path().join("publication");
    fs::create_dir(&publication).expect("publication root");
    fs::create_dir(publication.join("result.json")).expect("result blocker");
    let mut empty = request(FuzzProfile::Impacted, &publication);
    empty.changed_paths.push("docs/README.md".into());
    assert!(run(&empty, Path::new("missing"), "revision")
        .unwrap_err()
        .contains("publish fuzz artifact"));

    let execution = root.path().join("execution");
    assert!(run(
        &request(FuzzProfile::Smoke, &execution),
        Path::new("missing-compiler"),
        "revision"
    )
    .unwrap_err()
    .contains("oracle ended as infrastructure_error"));
    assert!(!execution.join("result.json").exists());
    assert_eq!(oracle_name(Oracle::None), "none");

    let serialization = attributed_serialization(Err(serde_json::Error::io(
        std::io::Error::other("rejected"),
    )))
    .expect_err("serialization failure");
    assert!(serialization.contains("serialize fuzz profile result"));

    let invalid = request(FuzzProfile::Impacted, &root.path().join("invalid"));
    assert!(run(&invalid, Path::new("missing"), "revision")
        .unwrap_err()
        .contains("--changed-path"));

    let result_path = root.path().join("serialized-result.json");
    let publication = publish_serialized_result(
        &result_path,
        Err(serde_json::Error::io(std::io::Error::other("rejected"))),
    )
    .expect_err("serialized publication failure");
    assert!(publication.contains("serialize fuzz profile result"));
}
