use std::fs;
use std::path::Path;

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn collect_python_sources(root: &Path, path: &Path, paths: &mut Vec<String>) {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let child = entry.path();
        let file_type = entry.file_type().unwrap();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if !matches!(
                child.file_name().and_then(|name| name.to_str()),
                Some(".git" | "target" | "build" | "_build" | "node_modules")
            ) {
                collect_python_sources(root, &child, paths);
            }
        } else if file_type.is_file()
            && child.extension().and_then(|extension| extension.to_str()) == Some("py")
        {
            paths.push(
                child
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

#[test]
fn repository_python_inventory_is_exhaustive() {
    let root = repository_root();
    let mut actual = Vec::new();
    collect_python_sources(root, root, &mut actual);
    actual.sort();

    let inventory =
        fs::read_to_string(root.join("docs/quality/TERLAN_SELF_VALIDATION_INVENTORY.tsv")).unwrap();
    let mut expected: Vec<_> = inventory
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(path, _)| path))
        .filter(|path| path.ends_with(".py"))
        .map(str::to_owned)
        .collect();
    expected.sort();

    assert_eq!(actual, expected, "Python source inventory drift");
}

#[test]
fn std_package_implementations_are_audited_without_build_outputs() {
    let root = repository_root();
    let mut files = Vec::new();
    super::collect_files(&root.join("std"), &mut files).unwrap();
    let paths: Vec<_> = files
        .iter()
        .map(|path| path.strip_prefix(root).unwrap().to_string_lossy())
        .collect();
    for required in [
        "std/lib.rs",
        "std/native/packages.rs",
        "std/db/native_operations.rs",
        "std/data/native/src/lib.rs",
        "std/net/native/src/lib.rs",
        "std/http/native/src/request.rs",
        "std/http/native/src/response_builder.rs",
        "std/http/native/src/source_descriptor/response.rs",
        "std/http/native/src/conversion_test.rs",
        "std/native/libpq/generated/native/rust/src/lib.rs",
    ] {
        assert!(
            paths.iter().any(|path| path == required),
            "missing {required}"
        );
    }
    assert!(!paths.iter().any(|path| path.starts_with("std/summaries/")));
    assert!(!paths
        .iter()
        .any(|path| path.contains("/target/") || path.contains("/_build/")));
}
