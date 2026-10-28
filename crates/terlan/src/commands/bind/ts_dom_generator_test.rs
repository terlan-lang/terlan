use super::*;

/// Compiler cache identities cannot change the reviewed binding manifest.
#[test]
fn reviewed_hashes_ignore_disposable_sidecars_but_track_api_changes() {
    let mut files = vec![
        GeneratedBindingFile {
            path: "std/summaries/std.js.Example.typi".into(),
            contents: "module std.js.Example. pub value(): Int.".into(),
        },
        GeneratedBindingFile {
            path: "std/summaries/std.js.Example.typi.deps".into(),
            contents: "old compiler fingerprint".into(),
        },
    ];
    let original = generated_file_hashes(&files).unwrap();
    assert_eq!(original.len(), 1);
    files[1].contents = "new compiler fingerprint".into();
    assert_eq!(generated_file_hashes(&files).unwrap(), original);
    files.pop();
    assert_eq!(generated_file_hashes(&files).unwrap(), original);
    files[0].contents = "module std.js.Example. pub value(): String.".into();
    assert_ne!(generated_file_hashes(&files).unwrap(), original);
}
