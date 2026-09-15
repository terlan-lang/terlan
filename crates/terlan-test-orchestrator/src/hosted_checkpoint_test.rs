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
    let excessive = (0..4097)
        .map(|id| format!("{hash}  ./subject-{id}\n"))
        .collect::<String>();
    assert!(manifest(excessive.as_bytes()).is_err());
}
