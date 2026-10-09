use super::*;

#[test]
fn checkpoint_manifest_requires_unique_canonical_bounded_subject_paths() {
    let hash = "a".repeat(64);
    let valid = format!("{hash}  ./context.json\n{hash}  ./coverage/suite.json\n");
    let parsed = manifest(valid.as_bytes()).unwrap();
    assert_eq!(parsed.len(), 2);
    assert!(required(&parsed, "context.json", &hash).is_ok());
    assert!(required(&parsed, "absent.json", &hash).is_err());
    assert!(required(&parsed, "context.json", &"b".repeat(64)).is_err());
    for path in [
        "",
        "/absolute",
        "../parent",
        "a/../b",
        "./same",
        "a//b",
        "a/",
        "a\\b",
        "a\tb",
    ] {
        assert!(
            manifest(format!("{hash}  ./{path}\n").as_bytes()).is_err(),
            "accepted {path:?}"
        );
    }
    assert!(manifest(format!("{hash}  ./{}\n", "a".repeat(4097)).as_bytes()).is_err());
    assert!(manifest(format!("{valid}{hash}  ./context.json\n").as_bytes()).is_err());
    assert!(manifest(format!("{}  ./context.json\n", "A".repeat(64)).as_bytes()).is_err());
    assert!(manifest(format!("{hash}  ./context.json").as_bytes()).is_err());
    assert!(manifest(b"").is_err());
    let excessive = (0..=MAX_MANIFEST_ENTRIES)
        .map(|id| format!("{hash}  ./subject-{id}\n"))
        .collect::<String>();
    assert!(manifest(excessive.as_bytes()).is_err());
}

#[test]
fn checkpoint_reader_accepts_distribution_sized_manifests() {
    let fixture = crate::test_orchestrator_test::temporary_fixture("distribution-checkpoint");
    let path = fixture.0.join("verified-files.sha256");
    let hash = "a".repeat(64);
    let prefix = format!("distribution/share/terlan/js/{}", "nested/".repeat(8));
    let contents = (0..10_000)
        .map(|id| format!("{hash}  ./{prefix}subject-{id}.js\n"))
        .collect::<String>();
    assert!(contents.len() > 1024 * 1024);
    std::fs::write(&path, contents).unwrap();
    let control = ProcessControl::new(std::time::Duration::from_secs(5));
    let entries = read_manifest(&path, control).unwrap();
    assert_eq!(entries.len(), 10_000);
    required(&entries, &format!("{prefix}subject-9999.js"), &hash).unwrap();
}

#[test]
fn checkpoint_reader_enforces_manifest_byte_and_entry_limits() {
    let fixture = crate::test_orchestrator_test::temporary_fixture("checkpoint-limits");
    let path = fixture.0.join("verified-files.sha256");
    let control = ProcessControl::new(std::time::Duration::from_secs(5));
    let hash = "a".repeat(64);
    let contents = (0..MAX_MANIFEST_ENTRIES)
        .map(|id| format!("{hash}  ./subject-{id}\n"))
        .collect::<String>();
    std::fs::write(&path, &contents).unwrap();
    assert_eq!(
        read_manifest(&path, control).unwrap().len(),
        MAX_MANIFEST_ENTRIES
    );
    std::fs::write(&path, format!("{contents}{hash}  ./extra\n")).unwrap();
    assert!(read_manifest(&path, control).is_err());
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_MANIFEST_BYTES + 1)
        .unwrap();
    assert!(read_manifest(&path, control)
        .unwrap_err()
        .detail
        .contains("byte budget"));
}
