use super::*;

fn project(kind: &str) -> tempfile::TempDir {
    fn copy(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("fixture directory");
        for entry in fs::read_dir(source).expect("fixture entries") {
            let entry = entry.expect("fixture entry");
            let target = destination.join(entry.file_name());
            if entry.file_type().expect("fixture type").is_dir() {
                copy(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).expect("fixture source");
            }
        }
    }
    let root = tempfile::tempdir().expect("external package root");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/path-interface-identity")
        .join(kind);
    copy(&source, root.path());
    root
}

fn test(root: &Path) -> ExitCode {
    run(
        CliCommand {
            verb: Some("test".into()),
            args: vec![root.join("app/tests").to_string_lossy().into_owned()],
        },
        CliState::default(),
    )
}

/// Uses external manifests, a real path dependency, and cold/warm generated summaries.
#[test]
fn path_dependency_keeps_process_entry_identity_with_unrelated_entry_provider() {
    let root = project("entry");
    for _ in 0..2 {
        assert_eq!(test(root.path()), ExitCode::SUCCESS);
    }
}

#[test]
fn path_dependency_keeps_same_named_input_and_output_provider_types() {
    let root = project("codec");
    for _ in 0..2 {
        assert_eq!(test(root.path()), ExitCode::SUCCESS);
    }
    // A real provider mismatch must remain rejected after the valid calls pass.
    let entry = root.path().join("app/src/app/Main.terl");
    let source = fs::read_to_string(&entry).expect("client source");
    assert!(source.contains("Store.restore()"));
    fs::write(
        entry,
        source
            .replace(
                "import fixture.Store.",
                "import fixture.Store.\nimport fixture.State.{Ready}.",
            )
            .replace("Store.restore()", "Ready(41)"),
    )
    .expect("wrong-provider client");
    assert_eq!(test(root.path()), ExitCode::from(1));
}

#[test]
fn path_dependency_patterns_preserve_single_tuple_constructor_aliases() {
    let root = project("tagged");
    for _ in 0..2 {
        assert_eq!(test(root.path()), ExitCode::SUCCESS);
    }
}
