use std::fs;

use super::output_cleanup::remove_stale_tvm_images;
use crate::support::test_fs;

/// Verifies native output cleanup removes legacy executable sidecars.
#[test]
fn native_output_cleanup_removes_json_and_reuse_sidecars() {
    let root = test_fs::temp_dir("output_cleanup", "legacy_sidecar_cleanup");
    let retained = root.join("app.tvm");
    let stale_image = root.join("old.tvm");
    let json_sidecar = root.join("app.tvm.json");
    let reuse_sidecar = root.join("app.tvm.reuse");
    let unrelated = root.join("notes.json");
    for path in [
        &retained,
        &stale_image,
        &json_sidecar,
        &reuse_sidecar,
        &unrelated,
    ] {
        fs::write(path, b"stale").expect("write cleanup fixture");
    }

    remove_stale_tvm_images(&root, Some("app.tvm")).expect("clean native output");

    assert!(retained.is_file());
    assert!(unrelated.is_file());
    assert!(!stale_image.exists());
    assert!(!json_sidecar.exists());
    assert!(!reuse_sidecar.exists());
    fs::remove_dir_all(root).expect("remove native cleanup fixture");
}
