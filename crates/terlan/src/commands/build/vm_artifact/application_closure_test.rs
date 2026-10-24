use super::*;
use std::fs;
use std::path::Path;

fn write(root: &Path, name: &str, source: &str) -> PathBuf {
    let path = root.join(format!("app/{name}.terl"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    path
}

#[test]
fn closure_follows_resolved_imports_and_excludes_unrelated_backends() {
    let root = tempfile::tempdir().unwrap();
    let mut paths = vec![
        write(root.path(), "Root", "module app.Root.\nimport app.{Middle, Shared}.\nimport type app.Types.Token.\npub value(): Int -> Middle.value() + Shared.value()."),
        write(root.path(), "Middle", "module app.Middle.\nimport app.Shared as Provider.\nimport app.Root.\npub value(): Int -> Provider.value()."),
        write(root.path(), "Shared", "module app.Shared.\npub value(): Int -> 3."),
        write(root.path(), "Types", "module app.Types.\npub type Token."),
        write(root.path(), "Browser", "module app.Browser.\nimport std.js.Array.\npub value(): Int -> 7."),
    ];
    let state = CliState {
        out_dir: root.path().join("out"),
        cache_dir: Some(root.path().join("cache")),
        incremental: true,
        ..CliState::default()
    };
    super::super::super::source_roots::prepare_source_root_interfaces(root.path(), &state).unwrap();
    for _ in 0..2 {
        let modules = compile_application_closure(&paths, "app.Root", &state)
            .unwrap_or_else(|_| panic!("compile project closure"));
        assert_eq!(
            modules
                .iter()
                .map(|module| module.compiled.core.module.as_str())
                .collect::<Vec<_>>(),
            ["app.Middle", "app.Root", "app.Shared", "app.Types"]
        );
        paths.reverse();
    }
    assert!(compile_application_closure(&paths, "app.Browser", &state).is_err());
}

#[test]
fn closure_rejects_missing_ambiguous_unreadable_and_malformed_sources() {
    let root = tempfile::tempdir().unwrap();
    let state = CliState::default();
    let valid = write(
        root.path(),
        "Root",
        "module app.Root.\npub value(): Int -> 1.",
    );
    let duplicate = write(
        root.path(),
        "Duplicate",
        "module app.Root.\npub value(): Int -> 2.",
    );
    let invalid = write(root.path(), "Invalid", "this is not a module");
    for (paths, entry, expected) in [
        (vec![valid.clone()], "app.Absent", "was not found"),
        (
            vec![valid, duplicate],
            "app.Root",
            "duplicate application module",
        ),
        (vec![invalid], "app.Root", "invalid application source"),
        (
            vec![root.path().join("absent.terl")],
            "app.Root",
            "absent.terl",
        ),
    ] {
        let error = compile_application_closure(&paths, entry, &state)
            .err()
            .expect("invalid input");
        let BuildOneError::Message(message) = error else {
            panic!("expected structural diagnostic")
        };
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn closure_does_not_hide_errors_in_reachable_modules() {
    let root = tempfile::tempdir().unwrap();
    let paths = vec![
        write(
            root.path(),
            "Root",
            "module app.Root.\nimport app.Bad.\npub value(): Int -> Bad.value().",
        ),
        write(
            root.path(),
            "Bad",
            "module app.Bad.\npub value(): Int -> \"wrong\".",
        ),
    ];
    let state = CliState {
        out_dir: root.path().join("out"),
        cache_dir: Some(root.path().join("cache")),
        ..CliState::default()
    };
    super::super::super::source_roots::prepare_source_root_interfaces(root.path(), &state).unwrap();
    assert!(compile_application_closure(&paths, "app.Root", &state).is_err());
}

#[test]
fn closure_rejects_reachable_asset_imports_before_dependency_traversal() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("asset.txt"), "asset").unwrap();
    let paths = vec![write(
        root.path(),
        "Root",
        "module app.Root.\nimport file \"../asset.txt\" as Asset.\npub value(): Int -> 1.",
    )];
    let state = CliState {
        out_dir: root.path().join("out"),
        cache_dir: Some(root.path().join("cache")),
        ..CliState::default()
    };
    super::super::super::source_roots::prepare_source_root_interfaces(root.path(), &state).unwrap();
    assert!(matches!(
        compile_application_closure(&paths, "app.Root", &state),
        Err(BuildOneError::Exit(_))
    ));
}
