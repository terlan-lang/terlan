use super::*;
use crate::{GenerationConfig, RunOptions};

fn generated_case(oracle: Oracle) -> GeneratedCase {
    let options = RunOptions::default();
    crate::generate(
        18,
        &GenerationConfig {
            seed: 1,
            max_depth: 8,
            oracle,
            timeout_ms: options.case_timeout_ms,
            rss_limit_bytes: options.rss_limit_bytes,
            cpu_limit_ms: options.cpu_limit_ms,
            compiler_revision: "test",
        },
    )
}

fn prepared_case(oracle: Oracle) -> (tempfile::TempDir, GeneratedCase) {
    let root = tempfile::tempdir().expect("package case root");
    let case = generated_case(oracle);
    super::super::artifacts::write_case(root.path(), &case).expect("case artifacts");
    (root, case)
}

#[test]
fn every_oracle_has_stable_package_spelling() {
    assert_eq!(oracle_name(Oracle::Check), "check");
    assert_eq!(oracle_name(Oracle::JavaScript), "js");
    assert_eq!(oracle_name(Oracle::Native), "native");
    assert_eq!(oracle_name(Oracle::None), "none");
}

#[cfg(unix)]
#[test]
fn path_dependency_consumer_records_success_and_failure() {
    let (root, case) = prepared_case(Oracle::Check);
    run(
        root.path(),
        &case,
        Path::new("/bin/true"),
        1_000,
        1 << 30,
        1_000,
    )
    .expect("path-dependency semantic comparison");
    let evidence =
        fs::read_to_string(root.path().join(ROUNDTRIP_ARTIFACT)).expect("package evidence");
    assert!(evidence.contains("in_workspace_and_path_dependency_accepted"));
    assert!(evidence.contains("\"path_dependency_outcome\":\"passed\""));
    assert!(evidence.contains("type_fuzz.case000018.Contract"));
    assert!(!fs::read_dir(root.path())
        .expect("case entries")
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".package-roundtrip-")));

    let error = run(
        root.path(),
        &case,
        Path::new("/bin/false"),
        1_000,
        1 << 30,
        1_000,
    )
    .expect_err("failed dependency compiler");
    assert!(error.contains("path-dependency semantic comparison ended as failed"));
    assert!(fs::read_to_string(root.path().join(ROUNDTRIP_ARTIFACT))
        .expect("failed package evidence")
        .contains("\"path_dependency_outcome\":\"failed\""));
}

#[test]
fn inert_cases_do_not_require_a_package_workspace() {
    let root = tempfile::tempdir().expect("case root");
    let mut no_modules = generated_case(Oracle::Check);
    no_modules.support_sources.clear();
    run(root.path(), &no_modules, Path::new("missing"), 1, 1, 1).expect("single-module case");
    run(
        root.path(),
        &generated_case(Oracle::None),
        Path::new("missing"),
        1,
        1,
        1,
    )
    .expect("offline case");
    assert!(!root.path().join(ROUNDTRIP_ARTIFACT).exists());
}

#[test]
fn staging_contains_only_consumer_and_provider_sources() {
    let (root, case) = prepared_case(Oracle::Check);
    let workspace = tempfile::tempdir().expect("package workspace");
    let consumer = stage(workspace.path(), root.path(), &case).expect("stage packages");
    let provider = workspace.path().join("provider");

    assert_eq!(
        fs::read_to_string(consumer.join(PROJECT_MANIFEST)).expect("consumer manifest"),
        CONSUMER_MANIFEST
    );
    assert_eq!(
        fs::read_to_string(provider.join(PROJECT_MANIFEST)).expect("provider manifest"),
        PROVIDER_MANIFEST
    );
    assert!(consumer.join("src").is_dir());
    assert!(consumer.join(ENTRY_SOURCE).is_file());
    assert!(!consumer.join(".terlan").exists());
    assert!(!provider.join(".terlan").exists());
    for source in &case.support_sources {
        assert_eq!(
            fs::read_to_string(provider.join(&source.path)).expect("provider source"),
            source.source
        );
    }
}

#[test]
fn staging_attributes_every_filesystem_boundary() {
    let (root, case) = prepared_case(Oracle::Check);

    let workspace_file = root.path().join("workspace-file");
    fs::write(&workspace_file, "blocked").expect("workspace blocker");
    assert!(run(&workspace_file, &case, Path::new("missing"), 1, 1, 1)
        .unwrap_err()
        .contains("create package round-trip workspace"));

    let missing_entry = tempfile::tempdir().expect("missing entry root");
    assert!(
        run(missing_entry.path(), &case, Path::new("missing"), 1, 1, 1,)
            .unwrap_err()
            .contains("copy package round-trip consumer entry")
    );
    assert!(stage(
        tempfile::tempdir().expect("entry workspace").path(),
        missing_entry.path(),
        &case,
    )
    .unwrap_err()
    .contains("copy package round-trip consumer entry"));

    let missing_provider = prepared_case(Oracle::Check);
    fs::remove_file(
        missing_provider
            .0
            .path()
            .join(&case.support_sources[0].path),
    )
    .expect("remove provider source");
    assert!(stage(
        tempfile::tempdir().expect("provider workspace").path(),
        missing_provider.0.path(),
        &missing_provider.1,
    )
    .unwrap_err()
    .contains("copy package round-trip provider source"));

    let mut blocked_parent = case.clone();
    blocked_parent.support_sources[0].path = PathBuf::from("terlan.toml/Nested.terl");
    assert!(stage(
        tempfile::tempdir()
            .expect("blocked parent workspace")
            .path(),
        root.path(),
        &blocked_parent,
    )
    .unwrap_err()
    .contains("create provider source directory"));

    for blocked in ["consumer", "provider"] {
        let layout = tempfile::tempdir().expect("blocked package layout");
        fs::write(layout.path().join(blocked), "blocked").expect("package blocker");
        let error = stage(layout.path(), root.path(), &case).expect_err("package must fail");
        assert!(error.contains(&format!("create package round-trip {blocked} package")));
    }

    let helper_root = tempfile::tempdir().expect("helper errors");
    let existing = helper_root.path().join("existing");
    fs::write(&existing, "file").expect("existing file");
    assert!(create_dir(&existing, "directory")
        .unwrap_err()
        .contains("create package round-trip directory"));
    let directory = helper_root.path().join("directory");
    fs::create_dir(&directory).expect("write blocker");
    assert!(write(&directory, "value", "manifest")
        .unwrap_err()
        .contains("write package round-trip manifest"));
    assert!(copy(
        &helper_root.path().join("missing"),
        &helper_root.path().join("destination"),
        "source",
    )
    .unwrap_err()
    .contains("copy package round-trip source"));

    for blocked in ["consumer", "provider"] {
        let layout = tempfile::tempdir().expect("blocked manifest layout");
        let consumer = layout.path().join("consumer");
        let provider = layout.path().join("provider");
        fs::create_dir(&consumer).expect("consumer package");
        fs::create_dir(&provider).expect("provider package");
        fs::create_dir(layout.path().join(blocked).join(PROJECT_MANIFEST))
            .expect("manifest blocker");
        let error = populate(&consumer, &provider, root.path(), &case)
            .expect_err("blocked manifest must fail");
        assert!(error.contains(&format!("write package round-trip {blocked} manifest")));
    }

    let layout = tempfile::tempdir().expect("blocked source layout");
    let consumer = layout.path().join("consumer");
    let provider = layout.path().join("provider");
    fs::create_dir(&consumer).expect("consumer package");
    fs::create_dir(&provider).expect("provider package");
    fs::write(consumer.join("src"), "blocked").expect("source blocker");
    assert!(populate(&consumer, &provider, root.path(), &case)
        .unwrap_err()
        .contains("create package round-trip consumer source root"));
}

#[cfg(unix)]
#[test]
fn invalid_modules_and_evidence_failures_are_attributed() {
    let (root, mut case) = prepared_case(Oracle::Check);
    case.support_sources[0].source = "not a module".into();
    assert!(run(root.path(), &case, Path::new("missing"), 1, 1, 1)
        .unwrap_err()
        .contains("has no valid module declaration"));

    let (_, valid_case) = prepared_case(Oracle::Check);
    fs::create_dir(root.path().join(format!("{ROUNDTRIP_ARTIFACT}.tmp")))
        .expect("publication blocker");
    assert!(run(
        root.path(),
        &valid_case,
        Path::new("/bin/true"),
        1_000,
        1 << 30,
        1_000,
    )
    .unwrap_err()
    .contains("write temporary fuzz artifact"));
}
