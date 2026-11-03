use super::*;

use crate::support::test_fs;

/// Creates an isolated temporary directory for clean command tests.
///
/// Inputs:
/// - `name`: stable test-specific suffix.
///
/// Output:
/// - Empty directory under the process temporary directory.
///
/// Transformation:
/// - Delegates to the shared test filesystem helper with the clean-command
///   namespace.
fn temp_dir(name: &str) -> PathBuf {
    test_fs::temp_dir("clean_command", name)
}

/// Verifies argument parsing defaults to the working directory.
///
/// Inputs:
/// - Empty command-local argument list.
///
/// Output:
/// - Parsed args pointing at `.`.
///
/// Transformation:
/// - Exercises parser defaults without touching the filesystem.
#[test]
fn parse_clean_args_defaults_to_current_directory() {
    assert_eq!(
        parse_clean_args(&[]).expect("clean args"),
        CleanArgs {
            project_dir: PathBuf::from(".")
        }
    );
}

/// Verifies argument parsing accepts one explicit project directory.
///
/// Inputs:
/// - One positional path.
///
/// Output:
/// - Parsed args pointing at that path.
///
/// Transformation:
/// - Exercises parser path forwarding without normalizing the user's spelling.
#[test]
fn parse_clean_args_accepts_project_directory() {
    assert_eq!(
        parse_clean_args(&["demo".to_string()]).expect("clean args"),
        CleanArgs {
            project_dir: PathBuf::from("demo")
        }
    );
}

/// Verifies argument parsing rejects options until they are designed.
///
/// Inputs:
/// - Unsupported option flag.
///
/// Output:
/// - Stable parser diagnostic.
///
/// Transformation:
/// - Prevents accidental option acceptance before the command supports scoped
///   cleaning.
#[test]
fn parse_clean_args_rejects_options() {
    assert_eq!(
        parse_clean_args(&["--all".to_string()]).expect_err("clean option error"),
        "terlc clean does not accept options yet"
    );
}

/// Verifies cleanup removes only compiler-owned generated outputs.
///
/// Inputs:
/// - Temporary project with `_build`, root and source-root generated summaries,
///   persistent cache state, source, tests, and a project manifest.
///
/// Output:
/// - Generated directories removed; source and manifest paths preserved.
///
/// Transformation:
/// - Runs the cleanup helper against a realistic scaffold shape.
#[test]
fn clean_project_removes_generated_outputs_only() {
    let root = temp_dir("generated_outputs");
    fs::create_dir_all(root.join("_build/bin")).expect("create build output");
    fs::create_dir_all(root.join(".terlan/tmp/cache")).expect("create scratch output");
    fs::create_dir_all(root.join(".terlan/test-results/terlan-vm"))
        .expect("create test result cache");
    fs::create_dir_all(root.join(".terlan/packages/registry")).expect("create package cache");
    fs::create_dir_all(root.join("src/app")).expect("create source");
    fs::create_dir_all(root.join("src/.terlan/packages/registry"))
        .expect("create source package cache");
    fs::create_dir_all(root.join("tests/app")).expect("create tests");
    fs::write(root.join("_build/bin/app"), "generated").expect("write generated artifact");
    fs::write(root.join(".terlan/tmp/cache/value"), "generated").expect("write scratch artifact");
    fs::write(
        root.join(".terlan/test-results/terlan-vm/result.passed"),
        "generated",
    )
    .expect("write test result cache");
    fs::write(root.join(".terlan/app.Main.typi"), "generated").expect("write summary");
    fs::write(root.join(".terlan/app.Main.typi.deps"), "generated")
        .expect("write summary dependencies");
    fs::write(root.join(".terlan/app.Main.typi.hash"), "generated").expect("write summary hash");
    fs::write(root.join(".terlan/settings.toml"), "persistent").expect("write cache settings");
    fs::write(root.join("src/.terlan/app.Source.typi"), "generated").expect("write source summary");
    fs::write(root.join("src/.terlan/app.Source.typi.deps"), "generated")
        .expect("write source summary dependencies");
    fs::write(root.join("src/.terlan/settings.toml"), "persistent")
        .expect("write source cache settings");
    fs::write(
        root.join("src/.terlan/packages/registry/package.typi"),
        "persistent source package cache",
    )
    .expect("write source package summary");
    fs::write(
        root.join(".terlan/packages/registry/package.typi"),
        "persistent package cache",
    )
    .expect("write package summary");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write manifest");
    fs::write(root.join("src/app/Main.terl"), "module app.Main.\n").expect("write source");
    fs::write(
        root.join("tests/app/MainTest.terl"),
        "module app.MainTest.\n",
    )
    .expect("write test");

    let removed = clean_project(&root).expect("clean project");

    assert_eq!(
        removed,
        vec![
            root.join("_build"),
            root.join(".terlan/tmp"),
            root.join(".terlan/test-results"),
            root.join(".terlan/app.Main.typi"),
            root.join(".terlan/app.Main.typi.deps"),
            root.join(".terlan/app.Main.typi.hash"),
            root.join("src/.terlan/app.Source.typi"),
            root.join("src/.terlan/app.Source.typi.deps"),
        ]
    );
    assert!(!root.join("_build").exists());
    assert!(!root.join(".terlan/tmp").exists());
    assert!(!root.join(".terlan/test-results").exists());
    assert!(root.join(".terlan").exists());
    assert!(root.join(".terlan/settings.toml").exists());
    assert!(root.join(".terlan/packages/registry/package.typi").exists());
    assert!(root.join("src/.terlan/settings.toml").exists());
    assert!(root
        .join("src/.terlan/packages/registry/package.typi")
        .exists());
    assert!(root.join("terlan.toml").exists());
    assert!(root.join("src/app/Main.terl").exists());
    assert!(root.join("tests/app/MainTest.terl").exists());

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

/// Verifies manifest source roots cannot redirect cleanup outside the project.
#[cfg(unix)]
#[test]
fn clean_project_rejects_source_root_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("source_root_escape");
    let external = temp_dir("source_root_escape_external");
    fs::create_dir_all(external.join(".terlan")).expect("create external cache");
    fs::write(external.join(".terlan/external.typi"), "preserve").expect("write external summary");
    symlink(&external, root.join("src")).expect("link source root");
    fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write manifest");

    let error = clean_project(&root).expect_err("reject escaped source root");

    assert!(error.contains("refusing to clean source root outside project"));
    assert!(external.join(".terlan/external.typi").exists());
    fs::remove_dir_all(root).expect("cleanup temp root");
    fs::remove_dir_all(external).expect("cleanup external root");
}

/// Verifies cleanup is successful when there is nothing to remove.
///
/// Inputs:
/// - Empty temporary project directory.
///
/// Output:
/// - Empty removed path list.
///
/// Transformation:
/// - Confirms absent generated outputs are treated as already clean.
#[test]
fn clean_project_skips_absent_outputs() {
    let root = temp_dir("already_clean");

    let removed = clean_project(&root).expect("clean project");

    assert!(removed.is_empty());
    fs::remove_dir_all(root).expect("cleanup temp dir");
}
