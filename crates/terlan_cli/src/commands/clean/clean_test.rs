use super::*;

use std::time::{SystemTime, UNIX_EPOCH};

/// Creates an isolated temporary directory for clean command tests.
///
/// Inputs:
/// - `name`: stable test-specific suffix.
///
/// Output:
/// - Empty directory under the process temporary directory.
///
/// Transformation:
/// - Adds process id and timestamp entropy, removes stale content, and creates
///   the requested root.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "terlan_clean_command_{name}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
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
/// - Temporary project with `_build`, `.terlan/tmp`, source, tests, and
///   manifest files.
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
    fs::create_dir_all(root.join("src/app")).expect("create source");
    fs::create_dir_all(root.join("tests/app")).expect("create tests");
    fs::write(root.join("_build/bin/app"), "generated").expect("write generated artifact");
    fs::write(root.join(".terlan/tmp/cache/value"), "generated").expect("write scratch artifact");
    fs::write(root.join("terlan.toml"), "[package]\nname = \"app\"\n").expect("write manifest");
    fs::write(root.join("src/app/Main.terl"), "module app.Main.\n").expect("write source");
    fs::write(
        root.join("tests/app/MainTest.terl"),
        "module app.MainTest.\n",
    )
    .expect("write test");

    let removed = clean_project(&root).expect("clean project");

    assert_eq!(removed, vec![root.join("_build"), root.join(".terlan/tmp")]);
    assert!(!root.join("_build").exists());
    assert!(!root.join(".terlan/tmp").exists());
    assert!(root.join(".terlan").exists());
    assert!(root.join("terlan.toml").exists());
    assert!(root.join("src/app/Main.terl").exists());
    assert!(root.join("tests/app/MainTest.terl").exists());

    fs::remove_dir_all(root).expect("cleanup temp dir");
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
