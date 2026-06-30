use std::path::{Path, PathBuf};

use super::{check_dir_phase_manifest_path, parse_check_args, validate_directory_module_layout};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn parse_check_args_accepts_source_path_only() {
    let parsed = parse_check_args(&args(&["src/app/Main.terl"])).expect("check args");

    assert_eq!(parsed, ("src/app/Main.terl".to_string(), None));
}

#[test]
fn parse_check_args_accepts_phase_manifest_path() {
    let parsed = parse_check_args(&args(&[
        "src/app/Main.terl",
        "--emit-phase-manifest",
        "_build/phase/Main.json",
    ]))
    .expect("check args");

    assert_eq!(
        parsed,
        (
            "src/app/Main.terl".to_string(),
            Some(PathBuf::from("_build/phase/Main.json"))
        )
    );
}

#[test]
fn parse_check_args_rejects_missing_or_duplicate_phase_manifest_path() {
    let missing = parse_check_args(&args(&["src/app/Main.terl", "--emit-phase-manifest"]))
        .expect_err("missing manifest path");
    assert_eq!(missing, "--emit-phase-manifest requires a path");

    let duplicate = parse_check_args(&args(&[
        "src/app/Main.terl",
        "--emit-phase-manifest",
        "one.json",
        "--emit-phase-manifest",
        "two.json",
    ]))
    .expect_err("duplicate manifest path");
    assert_eq!(duplicate, "duplicate --emit-phase-manifest");
}

#[test]
fn parse_check_args_rejects_missing_path_and_extra_positionals() {
    assert_eq!(
        parse_check_args(&[]).expect_err("missing path"),
        "missing path argument"
    );

    assert_eq!(
        parse_check_args(&args(&["src/app/Main.terl", "src/app/Other.terl"]))
            .expect_err("extra path"),
        "unexpected positional argument: src/app/Other.terl"
    );
}

#[test]
fn validate_directory_module_layout_accepts_path_derived_module_name() {
    let root = Path::new("src");
    let file = Path::new("src/app/Main.terl");

    assert_eq!(
        validate_directory_module_layout(root, file, "app.Main"),
        Ok(())
    );
}

#[test]
fn validate_directory_module_layout_reports_expected_module_name() {
    let root = Path::new("src");
    let file = Path::new("src/app/Main.terl");
    let error = validate_directory_module_layout(root, file, "app.Wrong").expect_err("layout");

    assert_eq!(
        error,
        "module declaration `app.Wrong` does not match source path `src/app/Main.terl`; expected `module app.Main.`"
    );
}

#[test]
fn check_dir_phase_manifest_path_uses_directory_root_for_extensionless_path() {
    assert_eq!(
        check_dir_phase_manifest_path(Path::new("_build/phase"), "app.Main"),
        PathBuf::from("_build/phase/app.Main.phase-manifest.json")
    );
}

#[test]
fn check_dir_phase_manifest_path_uses_file_stem_for_file_root() {
    assert_eq!(
        check_dir_phase_manifest_path(Path::new("_build/phase/check.json"), "app.Main"),
        PathBuf::from("_build/phase/check.app.Main.phase-manifest.json")
    );
}
