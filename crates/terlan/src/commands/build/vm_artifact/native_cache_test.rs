use std::fs;

use crate::support::test_fs;

use super::native_cache::{
    cache_manifest_bytes, load_verified_entry, publish_file, sha256_hex, CACHE_MANIFEST_NAME,
};

#[test]
fn native_cache_publication_replaces_complete_files_without_temporary_leaks() {
    let root = test_fs::temp_dir("native_cache", "atomic_publication");
    let path = root.join("module.tvm");

    publish_file(&path, b"first-complete-image").expect("publish first cache file");
    assert_eq!(
        fs::read(&path).expect("read first cache file"),
        b"first-complete-image"
    );

    publish_file(&path, b"replacement-image").expect("replace cache file");
    assert_eq!(
        fs::read(&path).expect("read replacement cache file"),
        b"replacement-image"
    );
    assert_eq!(
        fs::read_dir(&root)
            .expect("read cache directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.path() != path)
            .count(),
        0,
        "cache publication must not leave temporary files"
    );

    fs::remove_dir_all(root).expect("remove native cache test directory");
}

/// Proves cache admission binds the directory key, target, backend, manifest,
/// and every required payload before exposing an image.
#[test]
fn native_cache_rejects_poisoned_keys_target_drift_and_incomplete_publications() {
    let root = test_fs::temp_dir("native_cache", "verified_publication");
    let input = sha256_hex(b"expected native input");
    let directory = root.join(&input);
    let wrong_directory = root.join(sha256_hex(b"wrong cache key"));
    fs::create_dir_all(&directory).expect("create expected cache directory");
    fs::create_dir_all(&wrong_directory).expect("create poisoned cache directory");
    let object = b"complete object";
    let image = b"complete image";
    let names = ["module.o", "module.tvm"];

    fs::write(directory.join(names[0]), object).expect("publish object without manifest");
    assert!(load_verified_entry(
        &directory,
        &input,
        "x86_64-unknown-linux-gnu",
        "cranelift-test",
        &names,
        names[1],
    )
    .is_none());

    fs::write(directory.join(names[1]), image).expect("publish image without manifest");
    let manifest = cache_manifest_bytes(
        &input,
        "x86_64-unknown-linux-gnu",
        "cranelift-test",
        &[(names[0], object), (names[1], image)],
    );
    fs::write(directory.join(CACHE_MANIFEST_NAME), &manifest).expect("publish complete manifest");
    assert_eq!(
        load_verified_entry(
            &directory,
            &input,
            "x86_64-unknown-linux-gnu",
            "cranelift-test",
            &names,
            names[1],
        ),
        Some(image.to_vec())
    );
    fs::write(wrong_directory.join(names[0]), object).expect("copy object under wrong key");
    fs::write(wrong_directory.join(names[1]), image).expect("copy image under wrong key");
    fs::write(wrong_directory.join(CACHE_MANIFEST_NAME), &manifest)
        .expect("copy manifest under wrong key");

    for (label, cache_directory, expected_input, target, backend) in [
        (
            "poisoned key",
            &wrong_directory,
            input.as_str(),
            "x86_64-unknown-linux-gnu",
            "cranelift-test",
        ),
        (
            "wrong input",
            &directory,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "x86_64-unknown-linux-gnu",
            "cranelift-test",
        ),
        (
            "target drift",
            &directory,
            input.as_str(),
            "aarch64-unknown-linux-gnu",
            "cranelift-test",
        ),
        (
            "backend drift",
            &directory,
            input.as_str(),
            "x86_64-unknown-linux-gnu",
            "cranelift-other",
        ),
    ] {
        assert!(
            load_verified_entry(
                cache_directory,
                expected_input,
                target,
                backend,
                &names,
                names[1],
            )
            .is_none(),
            "accepted {label}"
        );
    }

    fs::remove_file(directory.join(names[0])).expect("remove required object");
    assert!(load_verified_entry(
        &directory,
        &input,
        "x86_64-unknown-linux-gnu",
        "cranelift-test",
        &names,
        names[1],
    )
    .is_none());
    fs::remove_dir_all(root).expect("remove verified cache fixture");
}
