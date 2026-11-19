use super::*;
use crate::test_orchestrator_test::temporary_fixture;

fn executable(root: &Path, name: &'static str, contents: &str) -> (&'static str, PathBuf) {
    let path = root.join(executable_name(name));
    fs::write(&path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (name, path)
}

fn programs(root: &Path, suffix: &str) -> Vec<(&'static str, PathBuf)> {
    PROGRAMS
        .iter()
        .map(|(name, _)| executable(root, name, &format!("{name}-{suffix}")))
        .collect()
}

#[test]
fn snapshots_are_content_addressed_and_independent_from_cargo_outputs() {
    let fixture = temporary_fixture("test-program-snapshot");
    let source = fixture.0.join("source");
    let target = fixture.0.join("target");
    fs::create_dir(&source).unwrap();
    let programs = programs(&source, "one");
    let first = snapshot(&target, &programs).unwrap();
    assert_eq!(snapshot(&target, &programs).unwrap(), first);
    for (name, source) in &programs {
        fs::write(source, format!("{name}-changed")).unwrap();
        assert_eq!(
            fs::read_to_string(first.join(executable_name(name))).unwrap(),
            format!("{name}-one")
        );
    }
    let second = snapshot(&target, &programs).unwrap();
    assert_ne!(second, first);
    for (name, _) in &programs {
        assert_eq!(
            fs::read_to_string(second.join(executable_name(name))).unwrap(),
            format!("{name}-changed")
        );
    }
}

#[test]
fn snapshot_rejects_invalid_inventories_and_repairs_owned_cache_files() {
    let fixture = temporary_fixture("test-program-snapshot-invalid");
    let source = fixture.0.join("source");
    let target = fixture.0.join("target");
    fs::create_dir(&source).unwrap();
    let programs = programs(&source, "valid");
    assert!(snapshot(&target, &programs[..2]).is_err());
    let duplicate = vec![
        programs[0].clone(),
        programs[0].clone(),
        programs[2].clone(),
    ];
    assert!(snapshot(&target, &duplicate).is_err());
    let directory = snapshot(&target, &programs).unwrap();
    let destination = directory.join(executable_name("terlc"));
    fs::write(&destination, "corrupt").unwrap();
    snapshot(&target, &programs).unwrap();
    assert_eq!(fs::read_to_string(destination).unwrap(), "terlc-valid");
}

fn report(directory: &Path) -> serde_json::Value {
    serde_json::json!({
        "decision": "pass",
        "executable_binding": {
            "verified": true,
            "before": PROGRAMS.iter().map(|(name, _)| serde_json::json!({
                "role": name,
                "path": directory.join(executable_name(name)),
            })).collect::<Vec<_>>(),
        },
    })
}

#[test]
fn completed_reports_route_only_one_verified_immutable_program_set() {
    let fixture = temporary_fixture("test-program-snapshot-report");
    let source = fixture.0.join("source");
    let target = fixture.0.join("target");
    fs::create_dir(&source).unwrap();
    let directory = snapshot(&target, &programs(&source, "report")).unwrap();
    let valid = report(&directory);
    assert_eq!(report_directory(&valid).unwrap(), directory);

    let mut failed = valid.clone();
    failed["decision"] = serde_json::json!("fail");
    assert!(report_directory(&failed).is_err());
    let mut mutable = valid.clone();
    mutable["executable_binding"]["before"][0]["path"] =
        serde_json::json!(target.join("debug/terlc"));
    assert!(report_directory(&mutable).is_err());
    let mut split = valid;
    split["executable_binding"]["before"][1]["path"] =
        serde_json::json!(target.join("validation-tools/test-programs-other/terlan-vm"));
    assert!(report_directory(&split).is_err());
}
