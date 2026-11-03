use std::path::{Path, PathBuf};

use super::{check_dir_phase_manifest_path, parse_check_args, run};
use crate::{CliCommand, CliState};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn parse_check_args_accepts_source_path_only() {
    let parsed = parse_check_args(&args(&["src/app/Main.terl"])).expect("check args");

    assert_eq!(parsed, ("src/app/Main.terl".to_string(), None, false));
}

#[test]
fn parse_check_args_accepts_interfaces_only_and_rejects_duplicates() {
    assert_eq!(
        parse_check_args(&args(&["--interfaces-only", "src"])).unwrap(),
        ("src".to_string(), None, true)
    );
    assert!(parse_check_args(&args(&["src", "--interfaces-only", "--interfaces-only"])).is_err());
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
            Some(PathBuf::from("_build/phase/Main.json")),
            false
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

#[test]
fn ordinary_directory_check_publishes_incremental_core_cache() {
    let root = crate::support::test_fs::temp_dir("check", "automatic_incremental_cache");
    let source_root = root.join("src");
    std::fs::create_dir_all(source_root.join("app")).expect("source root");
    std::fs::write(
        source_root.join("app/Main.terl"),
        "module app.Main.\npub answer(): Int -> 42.\n",
    )
    .expect("source");
    let cache = root.join("cache");
    let check = || {
        run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![source_root.to_string_lossy().into_owned()],
            },
            CliState {
                cache_dir: Some(cache.clone()),
                ..CliState::default()
            },
        )
    };
    assert_eq!(check(), std::process::ExitCode::SUCCESS);
    assert!(cache.join("app.Main.typi.deps").is_file());
    let checked_entries = std::fs::read_dir(cache.join("checked-v2"))
        .expect("checked cache buckets")
        .flatten()
        .flat_map(|bucket| {
            std::fs::read_dir(bucket.path().join("entries"))
                .into_iter()
                .flatten()
                .flatten()
        })
        .map(|entry| entry.path().join("checked.json"))
        .collect::<Vec<_>>();
    assert_eq!(checked_entries.len(), 1);
    let modified = std::fs::metadata(&checked_entries[0])
        .expect("checked payload")
        .modified()
        .expect("checked payload timestamp");
    assert_eq!(check(), std::process::ExitCode::SUCCESS);
    assert_eq!(
        std::fs::metadata(&checked_entries[0])
            .expect("unchanged checked payload")
            .modified()
            .expect("unchanged checked payload timestamp"),
        modified,
        "unchanged default checks must not republish checked CoreIR"
    );
    std::fs::remove_dir_all(root).expect("remove check fixture");
}
