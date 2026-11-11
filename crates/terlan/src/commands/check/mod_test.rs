use std::path::{Path, PathBuf};

use super::{check_dir_phase_manifest_path, parse_check_args, run};
use crate::{CliCommand, CliState};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn parse_check_args_accepts_source_path_only() {
    let parsed = parse_check_args(&args(&["src/app/Main.terl"])).expect("check args");

    assert_eq!(parsed, (vec!["src/app/Main.terl".to_string()], None, false));
}

#[test]
fn parse_check_args_accepts_interfaces_only_and_rejects_duplicates() {
    assert_eq!(
        parse_check_args(&args(&["--interfaces-only", "src"])).unwrap(),
        (vec!["src".to_string()], None, true)
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
            vec!["src/app/Main.terl".to_string()],
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
fn parse_check_args_requires_path_and_accepts_ordered_batch() {
    assert_eq!(
        parse_check_args(&[]).expect_err("missing path"),
        "missing path argument"
    );

    assert_eq!(
        parse_check_args(&args(&["src", "scripts/One.terls", "scripts/Two.terls"]))
            .expect("ordered paths"),
        (
            vec![
                "src".to_string(),
                "scripts/One.terls".to_string(),
                "scripts/Two.terls".to_string(),
            ],
            None,
            false,
        )
    );
}

#[test]
fn batch_check_reuses_project_graph_and_checks_only_selected_files() {
    let root = crate::support::test_fs::temp_dir("check", "project_script_batch");
    let source_root = root.join("src");
    let scripts = root.join("scripts");
    std::fs::create_dir_all(source_root.join("app")).expect("source root");
    std::fs::create_dir_all(&scripts).expect("scripts root");
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"batch-check\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\n",
    )
    .expect("project manifest");
    std::fs::write(
        source_root.join("app/Value.terl"),
        "module app.Value.\npub answer(): Int -> 42.\n",
    )
    .expect("project source");
    std::fs::write(
        source_root.join("app/Broken.terl"),
        "module app.Broken.\npub broken(): Int -> missing().\n",
    )
    .expect("unchanged invalid source");
    for name in ["One", "Two"] {
        std::fs::write(
            scripts.join(format!("{name}.terls")),
            "import app.Value.{answer}.\nassert(answer() == 42);\nUnit.\n",
        )
        .expect("project script");
    }
    let cache = root.join("cache");
    let manifests = root.join("phase");
    let result = run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source_root
                    .join("app/Value.terl")
                    .to_string_lossy()
                    .into_owned(),
                scripts.join("One.terls").to_string_lossy().into_owned(),
                scripts.join("Two.terls").to_string_lossy().into_owned(),
                "--emit-phase-manifest".to_string(),
                manifests.to_string_lossy().into_owned(),
            ],
        },
        CliState {
            cache_dir: Some(cache.clone()),
            ..CliState::default()
        },
    );

    assert_eq!(result, std::process::ExitCode::SUCCESS);
    assert!(manifests.join("app.Value.phase-manifest.json").is_file());
    assert!(manifests.join("One.phase-manifest.json").is_file());
    assert!(manifests.join("Two.phase-manifest.json").is_file());
    std::fs::remove_dir_all(root).expect("remove check fixture");
}

#[test]
fn batch_check_rejects_file_phase_manifest_destination() {
    let result = run(
        CliCommand {
            verb: Some("check".into()),
            args: args(&[
                "one.terl",
                "two.terl",
                "--emit-phase-manifest",
                "phase.json",
            ]),
        },
        CliState::default(),
    );

    assert_eq!(result, std::process::ExitCode::from(2));
}

#[test]
fn project_check_rejects_invalid_manifest_and_missing_source_root() {
    for (name, manifest) in [
        ("invalid_manifest", "not a manifest\n"),
        (
            "missing_source_root",
            "[package]\nname = \"missing\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"absent\"]\n",
        ),
    ] {
        let root = crate::support::test_fs::temp_dir("check", name);
        std::fs::write(root.join("terlan.toml"), manifest).expect("project manifest");
        let result = run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![root.to_string_lossy().into_owned()],
            },
            CliState::default(),
        );

        assert_eq!(result, std::process::ExitCode::from(1));
        std::fs::remove_dir_all(root).expect("remove check fixture");
    }
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
