use std::fs;

use crate::support::test_fs;

use super::native_cache::{
    cache_manifest_bytes, load_verified_entry, publish_file, publish_file_using, sha256_hex,
    TemporaryCacheFile, CACHE_MANIFEST_NAME,
};

#[test]
fn native_link_pin_survives_canonical_replacement_and_retirement() {
    let root = test_fs::TestDirectory::new("native_cache", "link_pin");
    let source = root.join("module.o");
    fs::write(&source, b"old immutable object").unwrap();
    let pin = TemporaryCacheFile::linked_beside(&source, &root.join("link-input.o")).unwrap();
    let pinned_path = pin.path().to_path_buf();
    publish_file(&source, b"replacement object").unwrap();
    assert_eq!(fs::read(pin.path()).unwrap(), b"old immutable object");
    fs::remove_file(&source).unwrap();
    assert_eq!(fs::read(pin.path()).unwrap(), b"old immutable object");
    drop(pin);
    assert!(!pinned_path.exists());
    root.close();
}

#[test]
fn native_link_pin_collision_preserves_the_other_owner() {
    let root = test_fs::TestDirectory::new("native_cache", "link_pin_collision");
    let source = root.join("module.o");
    let destination = root.join("other-owner.o");
    fs::write(&source, b"source object").unwrap();
    fs::write(&destination, b"another owner").unwrap();
    assert!(matches!(
        TemporaryCacheFile::reserve_link(&source, destination.clone()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(destination).unwrap(), b"another owner");
    assert_eq!(fs::read(source).unwrap(), b"source object");
    root.close();
}

#[cfg(unix)]
#[test]
fn native_link_pin_rejects_symlinks_without_adopting_the_destination() {
    let root = test_fs::TestDirectory::new("native_cache", "link_pin_symlink");
    let source = root.join("source.o");
    let indirect = root.join("indirect.o");
    let destination = root.join("link-input.o");
    fs::write(&source, b"source object").unwrap();
    std::os::unix::fs::symlink(&source, &indirect).unwrap();
    assert!(TemporaryCacheFile::reserve_link(&indirect, destination.clone()).is_err());
    assert!(!destination.exists());
    assert_eq!(fs::read(source).unwrap(), b"source object");
    root.close();
}

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

/// A replacement failure leaves the previous artifact byte-exact and removes
/// only the temporary file exclusively reserved by this publication attempt.
#[test]
fn native_cache_failed_replacement_preserves_previous_artifact() -> Result<(), String> {
    let root = test_fs::TestDirectory::new("native_cache", "failed_replacement");
    let path = root.join("module.tvm");
    fs::write(&path, b"verified-old-image").map_err(|error| error.to_string())?;
    let result = publish_file_using(&path, b"new-image", |temporary, destination| {
        assert_eq!(fs::read(temporary)?, b"new-image");
        assert_eq!(fs::read(destination)?, b"verified-old-image");
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected replacement failure",
        ))
    });
    assert!(result.is_err());
    assert_eq!(
        fs::read(&path).map_err(|error| error.to_string())?,
        b"verified-old-image"
    );
    assert_eq!(
        fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .count(),
        1
    );
    root.close();
    Ok(())
}

/// A colliding temporary path is never adopted or removed by the failed owner.
#[test]
fn native_cache_temporary_collision_preserves_foreign_bytes() -> Result<(), String> {
    let root = test_fs::TestDirectory::new("native_cache", "temporary_collision");
    let path = root.join(".module.tvm.123.0.tmp");
    fs::write(&path, b"another-owner").map_err(|error| error.to_string())?;
    let result = TemporaryCacheFile::reserve(path.clone());
    assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists));
    assert_eq!(
        fs::read(path).map_err(|error| error.to_string())?,
        b"another-owner"
    );
    root.close();
    Ok(())
}

/// Linker scratch is exclusively created before it is handed to a child tool.
#[test]
fn native_cache_temporary_reservation_owns_exactly_one_file() -> Result<(), String> {
    let root = test_fs::TestDirectory::new("native_cache", "temporary_ownership");
    let path = root.join(".module.tvm.tmp");
    let temporary = TemporaryCacheFile::reserve(path.clone()).map_err(|error| error.to_string())?;
    assert_eq!(fs::read(&path).map_err(|error| error.to_string())?, b"");
    fs::write(temporary.path(), b"partial-link-output").map_err(|error| error.to_string())?;
    drop(temporary);
    assert!(!path.exists());
    root.close();
    Ok(())
}

/// Replacement failure against a directory must not disturb its contents.
#[test]
fn native_cache_rejects_directory_destination_without_data_loss() -> Result<(), String> {
    let root = test_fs::TestDirectory::new("native_cache", "directory_destination");
    let path = root.join("module.tvm");
    fs::create_dir(&path).map_err(|error| error.to_string())?;
    fs::write(path.join("owned"), b"preserved").map_err(|error| error.to_string())?;
    assert!(publish_file(&path, b"new-image").is_err());
    assert_eq!(
        fs::read(path.join("owned")).map_err(|error| error.to_string())?,
        b"preserved"
    );
    assert_eq!(
        fs::read_dir(&root)
            .map_err(|error| error.to_string())?
            .count(),
        1
    );
    root.close();
    Ok(())
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
