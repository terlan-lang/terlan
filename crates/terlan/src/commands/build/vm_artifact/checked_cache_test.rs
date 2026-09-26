//! Tests for complete compiler-private checked implementation reuse.

use std::fs;
use std::path::Path;

use crate::commands::build::source_roots::prepare_source_root_interfaces;
use crate::support::test_fs;
use crate::CliState;

use super::checked_cache::checked_cache_file_for_test;
use super::compile::{compile_vm_module, CompiledVmModule};

/// Compiles one fixture through the VM checked-artifact frontend.
///
/// Inputs:
/// - `path`: fixture implementation source path.
/// - `state`: incremental compiler state shared by the fixture.
///
/// Output:
/// - Checked module result or the stable build error.
///
/// Transformation:
/// - Converts the test path to the command's UTF-8 path representation and
///   invokes the exact production compilation path.
pub(super) fn compile_fixture_module(
    path: &Path,
    state: &CliState,
) -> Result<CompiledVmModule, super::super::BuildOneError> {
    compile_vm_module(&path.to_string_lossy(), state)
}

/// Proves no-op and body-only builds reuse complete checked implementations.
#[test]
fn checked_cache_reuses_unchanged_dependency_implementations() {
    let fixture = CheckedCacheFixture::new("reuse_and_dependency_invalidation");
    fixture.prepare_interfaces();
    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(!fixture.compile_main().checked_cache_reused);

    fixture.prepare_interfaces();
    assert!(fixture.compile_dependency().checked_cache_reused);
    assert!(fixture.compile_main().checked_cache_reused);

    fixture.write_dependency("module app.Dependency.\n\npub value(): Int -> 2.\n");
    fixture.prepare_interfaces();
    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(
        fixture.compile_main().checked_cache_reused,
        "private dependency body changes must preserve consumer checked bodies"
    );

    fixture.write_dependency(
        "module app.Dependency.\n\npub value(): Int -> 2.\npub extra(): Int -> 3.\n",
    );
    fixture.prepare_interfaces();
    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(
        !fixture.compile_main().checked_cache_reused,
        "public dependency interface changes must invalidate consumers"
    );
    fixture.remove();
}

/// Proves poisoned and interface-only entries cannot execute as implementations.
#[test]
fn checked_cache_rejects_poisoned_or_missing_implementation_payloads() {
    let fixture = CheckedCacheFixture::new("poisoned_checked_payload");
    fixture.prepare_interfaces();
    assert!(!fixture.compile_dependency().checked_cache_reused);
    let source = fs::read_to_string(&fixture.dependency).expect("read dependency source");
    let checked_file = checked_cache_file_for_test(&source, &fixture.state)
        .expect("resolve checked cache payload");

    fs::write(&checked_file, b"not checked CoreIR").expect("poison checked payload");
    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(fixture.compile_dependency().checked_cache_reused);

    let hostile_depth = format!(
        "{}0{}",
        "[".repeat(crate::support::MAX_JSON_NESTING_DEPTH + 1),
        "]".repeat(crate::support::MAX_JSON_NESTING_DEPTH + 1)
    );
    fs::write(&checked_file, hostile_depth).expect("write hostile-depth checked payload");
    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(fixture.compile_dependency().checked_cache_reused);

    fs::remove_file(&checked_file).expect("remove checked implementation payload");
    assert!(fixture.cache.join("app.Dependency.typi").is_file());
    assert!(fixture.cache.join("app.Dependency.typi.deps").is_file());
    assert!(
        !fixture.compile_dependency().checked_cache_reused,
        "interface summaries must never serve as executable implementations"
    );
    assert!(fixture.compile_dependency().checked_cache_reused);
    fixture.remove();
}

/// Proves a direct incremental file build can publish into a cold cache root.
#[test]
fn checked_cache_publisher_creates_missing_cache_root() {
    let fixture = CheckedCacheFixture::new("cold_cache_root");
    assert!(!fixture.cache.exists());

    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(fixture.cache.join("app.Dependency.typi.deps").is_file());
    assert!(fixture.compile_dependency().checked_cache_reused);
    fixture.remove();
}

#[test]
fn checked_cache_retention_preserves_the_current_reader_under_entry_pressure() {
    use super::artifact_cache_retention::{Budget, CacheFamily, RetainedCache};
    use std::collections::BTreeSet;

    let fixture = CheckedCacheFixture::new("retention_entry_pressure");
    fixture.prepare_interfaces();
    assert!(!fixture.compile_dependency().checked_cache_reused);
    let source = fs::read_to_string(&fixture.dependency).unwrap();
    let checked = checked_cache_file_for_test(&source, &fixture.state).unwrap();
    let generation = checked.parent().unwrap();
    let entries = generation.parent().unwrap();
    let bucket = entries.parent().unwrap();
    let prefix = bucket.file_name().unwrap().to_str().unwrap();
    let keys = (0..256)
        .map(|value| format!("{prefix}{value:063x}"))
        .collect::<BTreeSet<_>>();
    let cache = RetainedCache::open(
        bucket,
        CacheFamily::CheckedImplementations,
        keys.clone(),
        Budget {
            entries: 512,
            ..Budget::default()
        },
    )
    .unwrap();
    for key in keys {
        cache
            .publish(
                &key,
                "checked.json",
                b"obsolete serialized implementation",
                b"obsolete manifest",
            )
            .unwrap();
    }
    drop(cache);
    assert_eq!(fs::read_dir(entries).unwrap().count(), 257);
    assert!(fixture.compile_dependency().checked_cache_reused);
    assert!(checked.is_file());
    assert_eq!(fs::read_dir(entries).unwrap().count(), 256);
    fixture.remove();
}

#[test]
fn checked_cache_retention_errors_are_not_silently_reclassified_as_misses() {
    let fixture = CheckedCacheFixture::new("retention_unowned_file");
    assert!(!fixture.compile_dependency().checked_cache_reused);
    let source = fs::read_to_string(&fixture.dependency).unwrap();
    let checked = checked_cache_file_for_test(&source, &fixture.state).unwrap();
    let unowned = checked.parent().unwrap().join("source.terl");
    fs::write(&unowned, b"do not adopt this source").unwrap();
    let result = compile_fixture_module(&fixture.dependency, &fixture.state);
    assert!(
        matches!(result, Err(super::super::BuildOneError::Message(message)) if message.contains("build.cache.retention"))
    );
    assert_eq!(fs::read(unowned).unwrap(), b"do not adopt this source");
    fixture.remove();
}

#[test]
fn checked_cache_does_not_adopt_legacy_unleased_storage() {
    let fixture = CheckedCacheFixture::new("retention_legacy_namespace");
    let legacy = fixture.cache.join("checked");
    fs::create_dir_all(&legacy).unwrap();
    let unrelated = legacy.join("source.terl");
    fs::write(&unrelated, b"old compiler owns this namespace").unwrap();
    assert!(!fixture.compile_dependency().checked_cache_reused);
    assert!(fixture.compile_dependency().checked_cache_reused);
    assert_eq!(
        fs::read(unrelated).unwrap(),
        b"old compiler owns this namespace"
    );
    fixture.remove();
}

/// Complete two-module checked-cache fixture.
struct CheckedCacheFixture {
    /// Temporary fixture root removed after each test.
    root: std::path::PathBuf,
    /// Source root passed through the interface preparation stage.
    source_root: std::path::PathBuf,
    /// Dependency implementation edited by invalidation tests.
    dependency: std::path::PathBuf,
    /// Consumer implementation whose dependency hashes are validated.
    main: std::path::PathBuf,
    /// Compiler-private cache root inspected by corruption tests.
    cache: std::path::PathBuf,
    /// Incremental compiler configuration shared by fixture operations.
    state: CliState,
}

impl CheckedCacheFixture {
    /// Creates one isolated dependency and consumer project.
    fn new(name: &str) -> Self {
        let root = test_fs::temp_dir("checked_implementation_cache", name);
        let source_root = root.join("src");
        let module_dir = source_root.join("app");
        let dependency = module_dir.join("Dependency.terl");
        let main = module_dir.join("Main.terl");
        let cache = root.join("cache");
        fs::create_dir_all(&module_dir).expect("create module fixture directory");
        fs::write(
            &dependency,
            "module app.Dependency.\n\npub value(): Int -> 1.\n",
        )
        .expect("write dependency fixture");
        fs::write(
            &main,
            "module app.Main.\n\nimport app.Dependency.{value}.\n\npub main(): Int -> value().\n",
        )
        .expect("write consumer fixture");
        let state = CliState {
            incremental: true,
            cache_dir: Some(cache.clone()),
            out_dir: root.join("build"),
            ..CliState::default()
        };
        Self {
            root,
            source_root,
            dependency,
            main,
            cache,
            state,
        }
    }

    /// Prepares project-local interface summaries using production logic.
    fn prepare_interfaces(&self) {
        prepare_source_root_interfaces(&self.source_root, &self.state)
            .expect("prepare fixture interfaces");
    }

    /// Compiles the dependency and returns its cache-observable result.
    fn compile_dependency(&self) -> CompiledVmModule {
        compile_fixture_module(&self.dependency, &self.state).expect("compile dependency fixture")
    }

    /// Compiles the consumer and returns its cache-observable result.
    fn compile_main(&self) -> CompiledVmModule {
        compile_fixture_module(&self.main, &self.state).expect("compile consumer fixture")
    }

    /// Replaces dependency source for invalidation scenarios.
    fn write_dependency(&self, source: &str) {
        fs::write(&self.dependency, source).expect("replace dependency fixture");
    }

    /// Removes the complete fixture tree.
    fn remove(self) {
        fs::remove_dir_all(self.root).expect("remove checked cache fixture");
    }
}
