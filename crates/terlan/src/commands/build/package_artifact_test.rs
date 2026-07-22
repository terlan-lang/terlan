//! Tests for immutable target package artifact verification and caching.

use super::*;
use crate::support::test_fs;
use serde_json::json;
use std::io::Write;

/// Imports, resolves, and revalidates a complete local artifact archive.
#[test]
fn artifact_import_is_content_addressed_and_runtime_ready() {
    let root = test_fs::temp_dir("package_artifact", "import");
    let archive = write_test_artifact(&root, "x86_64-unknown-linux-gnu");
    let cache = root.join("cache");

    let entry = import_artifact(&cache, &archive, "x86_64-unknown-linux-gnu")
        .expect("import valid artifact");
    let resolved = validate_cached_artifact(&cache, &entry).expect("resolve cached artifact");

    assert_eq!(entry.package, "vision");
    assert_eq!(entry.target, "x86_64-unknown-linux-gnu");
    assert!(resolved.package_dir.join("terlan.toml").is_file());
    assert_eq!(
        resolved.environment,
        vec![(
            "TERLAN_NATIVE_BOUNDARY_HELPER_PATH".to_string(),
            resolved.root.join("bin/guard")
        )]
    );
    assert!(cache
        .join("artifacts/vision/1.0.0/x86_64-unknown-linux-gnu")
        .join(&entry.cache_key)
        .join("archive.tar.zst")
        .is_file());
}

/// Rejects an artifact selected for a different platform before publication.
#[test]
fn artifact_import_rejects_target_mismatch() {
    let root = test_fs::temp_dir("package_artifact", "target_mismatch");
    let archive = write_test_artifact(&root, "x86_64-unknown-linux-gnu");

    let error = import_artifact(&root.join("cache"), &archive, "aarch64-unknown-linux-gnu")
        .expect_err("wrong target must fail");

    assert!(error.contains("error[package_artifact_target_mismatch]"));
}

/// Rejects mutation of a cached archive even when extracted files remain intact.
#[test]
fn cached_artifact_rejects_archive_mutation() {
    let root = test_fs::temp_dir("package_artifact", "archive_mutation");
    let archive = write_test_artifact(&root, "x86_64-unknown-linux-gnu");
    let cache = root.join("cache");
    let entry =
        import_artifact(&cache, &archive, "x86_64-unknown-linux-gnu").expect("import artifact");
    let cached_archive = artifact_cache_dir(
        &cache,
        &entry.package,
        &entry.version,
        &entry.target,
        &entry.cache_key,
    )
    .join("archive.tar.zst");
    fs::OpenOptions::new()
        .append(true)
        .open(cached_archive)
        .expect("open cached archive")
        .write_all(b"mutation")
        .expect("mutate cached archive");

    let error = validate_cached_artifact(&cache, &entry).expect_err("mutation must fail");

    assert!(error.contains("error[package_artifact_checksum_mismatch]"));
}

/// Covers target defaults and explicit validation independently of extraction.
#[test]
fn active_target_accepts_supported_explicit_triples() {
    assert_eq!(
        active_artifact_target(Some("aarch64-unknown-linux-gnu")),
        Ok("aarch64-unknown-linux-gnu".to_string())
    );
    assert!(active_artifact_target(Some("../target")).is_err());
}

/// Creates one minimal valid package artifact fixture.
pub(super) fn write_test_artifact(root: &Path, target: &str) -> PathBuf {
    write_named_test_artifact(root, target, "vision", "1.0.0")
}

/// Creates one valid artifact fixture for a named locked package.
pub(crate) fn write_named_test_artifact(
    root: &Path,
    target: &str,
    package: &str,
    version: &str,
) -> PathBuf {
    let staging = root.join("staging/vision-artifact");
    fs::create_dir_all(staging.join("package/src").join(package)).expect("create package");
    fs::create_dir_all(staging.join("bin")).expect("create bin");
    fs::create_dir_all(staging.join("lib")).expect("create lib");
    fs::write(
        staging.join("package/terlan.toml"),
        format!(
            "[package]\nname = \"{package}\"\nversion = \"{version}\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n"
        ),
    )
    .expect("write package manifest");
    fs::write(
        staging.join("package/src").join(package).join("Util.terl"),
        format!("module {package}.Util.\n\npub one(): Int ->\n    1.\n"),
    )
    .expect("write package source");
    fs::write(staging.join("bin/guard"), "guard").expect("write guard");
    fs::write(staging.join("bin/worker"), "worker").expect("write worker");
    let manifest = json!({
        "schema": "terlan.vision.artifact.v1",
        "target": target,
        "package": { "name": package, "version": version },
        "terlan_package": "package",
        "runtime": {
            "guard": "bin/guard",
            "worker": "bin/worker",
            "library_dir": "lib",
            "environment": {
                "TERLAN_NATIVE_BOUNDARY_HELPER_PATH": "bin/guard"
            }
        }
    });
    fs::write(
        staging.join("artifact.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest).expect("serialize manifest")
        ),
    )
    .expect("write artifact manifest");
    let mut checksums = String::new();
    for path in collect_payload_paths(&staging).expect("collect payload") {
        checksums.push_str(&format!(
            "{}  {}\n",
            hash_path(&staging.join(&path)).expect("hash payload"),
            path.display()
        ));
    }
    fs::write(staging.join("checksums.sha256"), checksums).expect("write checksums");

    let archive_path = root.join("vision.tar.zst");
    let archive_file = fs::File::create(&archive_path).expect("create archive");
    let encoder = zstd::stream::write::Encoder::new(archive_file, 3).expect("create encoder");
    let mut archive = tar::Builder::new(encoder);
    archive
        .append_dir_all("vision-artifact", &staging)
        .expect("append artifact");
    let encoder = archive.into_inner().expect("finish tar");
    encoder.finish().expect("finish zstd");
    archive_path
}
