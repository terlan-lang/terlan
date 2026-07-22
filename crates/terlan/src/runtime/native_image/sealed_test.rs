use std::fs;

use sha2::{Digest, Sha256};

use super::*;

/// Allocates one isolated filesystem fixture.
fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "terlan-sealed-image-test-{}-{}-{name}",
        std::process::id(),
        NEXT_SEAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).expect("create sealed-image fixture");
    path
}

#[test]
fn sidecar_rejection_covers_every_retired_metadata_name() {
    for suffix in ["json", "tvm.json", "reuse"] {
        let root = fixture(suffix);
        let image = root.join("app.tvm");
        fs::write(&image, b"native").expect("write image placeholder");
        let sidecar = if suffix == "reuse" {
            root.join("app.tvm.reuse")
        } else {
            image.with_extension(suffix)
        };
        fs::write(&sidecar, b"mutable").expect("write sidecar");
        let error = reject_tvm_image_sidecars(&image).expect_err("sidecar must fail");
        assert!(
            error.contains("tvm.image.sidecar"),
            "unexpected error: {error}"
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }
}

#[test]
fn sealed_digest_detects_private_copy_mutation() {
    let root = fixture("digest");
    let image = root.join("admitted.tvm");
    fs::write(&image, b"first").expect("write first image");
    let digest: [u8; 32] = Sha256::digest(b"first").into();
    verify_path_digest(&image, digest).expect("unchanged image");
    fs::write(&image, b"second").expect("replace private image");
    let error = verify_path_digest(&image, digest).expect_err("mutation must fail");
    assert!(error.contains("tvm.image.seal_changed"));
    fs::remove_dir_all(root).expect("remove fixture");
}

#[test]
fn private_seal_directories_are_unique() {
    let first = create_private_root().expect("first private root");
    let second = create_private_root().expect("second private root");
    assert_ne!(first, second);
    assert!(first.starts_with(std::env::temp_dir()));
    assert!(second.starts_with(std::env::temp_dir()));
    fs::remove_dir(first).expect("remove first root");
    fs::remove_dir(second).expect("remove second root");
}

#[cfg(unix)]
#[test]
fn private_seal_directory_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let root = create_private_root().expect("private root");
    let mode = fs::metadata(&root)
        .expect("private root metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    fs::remove_dir(root).expect("remove private root");
}
