use std::fs;
use std::hash::{DefaultHasher, Hasher};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use super::VmSourceReloadAdapter;
use crate::runtime::vm::code_server::{
    VmCodeServerEvent, VmModuleArtifact, VmModuleGenerationId, VmModuleGenerationState,
    VmStagedModuleArtifact,
};

/// Creates an isolated source-reload test directory.
///
/// Inputs:
/// - `name`: readable fixture name.
///
/// Output:
/// - Created temporary directory path.
///
/// Transformation:
/// - Mixes process and clock state into the directory name so parallel tests do
///   not share changed source files.
fn source_reload_test_dir(name: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    hasher.write_usize(std::process::id() as usize);
    hasher.write(
        &std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
            .to_le_bytes(),
    );
    let dir = std::env::temp_dir().join(format!(
        "terlan_vm_source_reload_{name}_{}",
        hasher.finish()
    ));
    fs::create_dir_all(&dir).expect("create source reload test dir");
    dir
}

fn published_event(event: &VmCodeServerEvent) -> Option<(&str, VmModuleGenerationId)> {
    match event {
        VmCodeServerEvent::Published { module, generation } => Some((module.as_str(), *generation)),
        _ => None,
    }
}

fn hot_reloaded_event(
    event: &VmCodeServerEvent,
) -> Option<(
    VmModuleGenerationId,
    VmModuleGenerationState,
    VmModuleGenerationId,
)> {
    match event {
        VmCodeServerEvent::HotReloaded {
            previous_generation,
            previous_state,
            active_generation,
            ..
        } => Some((*previous_generation, *previous_state, *active_generation)),
        _ => None,
    }
}

/// Verifies changed Terlan files publish and hot reload VM generations.
///
/// Inputs:
/// - One `.terl` file written twice with the same module name and changed body.
///
/// Output:
/// - Initial publish event and later hot-reload event.
///
/// Transformation:
/// - Exercises the concrete file-to-code-server adapter that a dev watcher can
///   call when a source file changes.
#[test]
fn source_reload_adapter_publishes_changed_terlan_file_generations() {
    let dir = source_reload_test_dir("publish_reload");
    let path = dir.join("app.terl");
    let mut adapter = VmSourceReloadAdapter::new();

    fs::write(&path, "module app.\n\npub value(): Int ->\n    1.\n").expect("write first source");
    let first = adapter
        .publish_changed_file(&path)
        .expect("first publish should succeed")
        .expect("terlan file should publish");
    let (module, first_generation) =
        published_event(&first).expect("expected initial source publish");
    assert_eq!(module, "app");

    fs::write(&path, "module app.\n\npub value(): Int ->\n    2.\n").expect("write changed source");
    let second = adapter
        .publish_changed_file(&path)
        .expect("hot reload should succeed")
        .expect("terlan file should hot reload");
    let (previous_generation, previous_state, active_generation) =
        hot_reloaded_event(&second).expect("expected source hot reload");

    assert!(published_event(&second).is_none());
    assert!(hot_reloaded_event(&first).is_none());
    assert_eq!(previous_generation, first_generation);
    assert_eq!(previous_state, VmModuleGenerationState::Retired);
    assert_ne!(active_generation, first_generation);

    let events = adapter.event_snapshots();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event, first);
    assert_eq!(events[1].event, second);

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies non-Terlan paths do not publish source generations.
///
/// Inputs:
/// - One changed text asset path.
///
/// Output:
/// - `None` and no code-server events.
///
/// Transformation:
/// - Keeps asset-reload and VM source-reload responsibilities separate so a
///   future watcher can dispatch events by path type without false publishes.
#[test]
fn source_reload_adapter_ignores_non_terlan_paths() {
    let dir = source_reload_test_dir("ignore_assets");
    let path = dir.join("style.css");
    fs::write(&path, "body { color: black; }\n").expect("write asset");

    let mut adapter = VmSourceReloadAdapter::new();
    let event = adapter
        .publish_changed_file(&path)
        .expect("non-source path should not fail");

    assert_eq!(event, None);
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies watcher-style path batches publish only Terlan source files.
///
/// Inputs:
/// - One asset path and two Terlan source paths in a mixed event batch.
///
/// Output:
/// - Ordered publish events for the two source modules only.
///
/// Transformation:
/// - Gives future `terlc dev` and filesystem watcher wiring a stable adapter
///   surface that can process noisy event batches without treating asset
///   reloads as VM source generations.
#[test]
fn source_reload_adapter_publishes_only_sources_from_mixed_batch() {
    let dir = source_reload_test_dir("mixed_batch");
    let asset = dir.join("style.css");
    let first = dir.join("First.terl");
    let second = dir.join("Second.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&first, "module first.\n\npub value(): Int ->\n    1.\n").expect("write first");
    fs::write(&second, "module second.\n\npub value(): Int ->\n    2.\n").expect("write second");

    let mut adapter = VmSourceReloadAdapter::new();
    let events = adapter
        .publish_changed_files(&[asset, first, second])
        .expect("mixed batch should publish source files");

    assert_eq!(events.len(), 2);
    let (first_module, first_generation) =
        published_event(&events[0]).expect("first publish event");
    let (second_module, second_generation) =
        published_event(&events[1]).expect("second publish event");
    assert_eq!(first_module, "first");
    assert_eq!(first_generation.as_u64(), 1);
    assert_eq!(second_module, "second");
    assert_eq!(second_generation.as_u64(), 2);
    assert_eq!(adapter.event_snapshots().len(), 2);

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies mixed batch publication reports inspectable reload diagnostics.
///
/// Inputs:
/// - One asset path, two unique Terlan source paths, and one duplicate source
///   path.
///
/// Output:
/// - A batch report with path-classification counts and the two publish events.
///
/// Transformation:
/// - Locks the debug surface needed by HTTP/dev hot reload without exposing the
///   mutable code-server table or filesystem watcher implementation details.
#[test]
fn source_reload_adapter_reports_mixed_batch_diagnostics() {
    let dir = source_reload_test_dir("mixed_batch_report");
    let asset = dir.join("style.css");
    let first = dir.join("First.terl");
    let second = dir.join("Second.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&first, "module first.\n\npub value(): Int ->\n    1.\n").expect("write first");
    fs::write(&second, "module second.\n\npub value(): Int ->\n    2.\n").expect("write second");

    let mut adapter = VmSourceReloadAdapter::new();
    let report = adapter
        .publish_changed_files_with_report(&[asset, first.clone(), second, first])
        .expect("mixed batch report should publish source files");

    assert_eq!(report.changed_paths, 4);
    assert_eq!(report.unique_source_paths, 2);
    assert_eq!(report.ignored_paths, 1);
    assert_eq!(report.duplicate_source_paths, 1);
    assert_eq!(report.events.len(), 2);

    let (first_module, first_generation) =
        published_event(&report.events[0]).expect("first publish event");
    let (second_module, second_generation) =
        published_event(&report.events[1]).expect("second publish event");
    assert_eq!(first_module, "first");
    assert_eq!(first_generation.as_u64(), 1);
    assert_eq!(second_module, "second");
    assert_eq!(second_generation.as_u64(), 2);
    assert_eq!(adapter.event_snapshots().len(), 2);

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies invalid mixed batches cannot partially publish valid sources.
///
/// Inputs:
/// - One asset path, one valid Terlan source path, and one malformed Terlan
///   source path.
///
/// Output:
/// - Stable compile error and no code-server events.
///
/// Transformation:
/// - Locks watcher/dev reload batches as compile-before-publish transactions
///   so a bad source edit cannot leave the VM with only part of a changed
///   batch loaded.
#[test]
fn source_reload_adapter_rejects_invalid_mixed_batch_without_partial_publication() {
    let dir = source_reload_test_dir("invalid_mixed_batch");
    let asset = dir.join("style.css");
    let valid = dir.join("Valid.terl");
    let broken = dir.join("Broken.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&valid, "module valid.\n\npub value(): Int ->\n    1.\n").expect("write valid");
    fs::write(&broken, "module broken.\n\npub value(: Int ->\n    2.\n").expect("write broken");

    let mut adapter = VmSourceReloadAdapter::new();
    let error = adapter
        .publish_changed_files(&[asset, valid, broken])
        .expect_err("invalid batch should fail");

    assert!(
        error.contains("source hot reload compile failed"),
        "unexpected error: {error}"
    );
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies report-producing reload batches remain atomic on compile failure.
///
/// Inputs:
/// - One valid Terlan source and one malformed Terlan source.
///
/// Output:
/// - Stable compile error and no code-server events.
///
/// Transformation:
/// - Ensures debug-report generation cannot weaken the existing
///   compile-before-publish transaction boundary.
#[test]
fn source_reload_adapter_report_rejects_invalid_batch_without_partial_publication() {
    let dir = source_reload_test_dir("invalid_report_batch");
    let valid = dir.join("Valid.terl");
    let broken = dir.join("Broken.terl");
    fs::write(&valid, "module valid.\n\npub value(): Int ->\n    1.\n").expect("write valid");
    fs::write(&broken, "module broken.\n\npub value(: Int ->\n    2.\n").expect("write broken");

    let mut adapter = VmSourceReloadAdapter::new();
    let error = adapter
        .publish_changed_files_with_report(&[valid, broken])
        .expect_err("invalid report batch should fail");

    assert!(
        error.contains("source hot reload compile failed"),
        "unexpected error: {error}"
    );
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies unreadable mixed batches cannot partially publish valid sources.
///
/// Inputs:
/// - One asset path, one valid Terlan source path, and one missing Terlan
///   source path.
///
/// Output:
/// - Stable read error and no code-server events.
///
/// Transformation:
/// - Keeps filesystem failure atomic across watcher/dev reload batches, not
///   only compile failure.
#[test]
fn source_reload_adapter_rejects_unreadable_mixed_batch_without_partial_publication() {
    let dir = source_reload_test_dir("unreadable_mixed_batch");
    let asset = dir.join("style.css");
    let valid = dir.join("Valid.terl");
    let missing = dir.join("Missing.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&valid, "module valid.\n\npub value(): Int ->\n    1.\n").expect("write valid");

    let mut adapter = VmSourceReloadAdapter::new();
    let error = adapter
        .publish_changed_files(&[asset, valid, missing])
        .expect_err("unreadable batch should fail");

    assert!(
        error.contains("failed to read changed Terlan source"),
        "unexpected error: {error}"
    );
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies duplicate watcher events for one source publish once per batch.
///
/// Inputs:
/// - One Terlan source path repeated multiple times with one unrelated asset.
///
/// Output:
/// - A single publish event for the source module.
///
/// Transformation:
/// - Prevents noisy filesystem watchers from creating redundant hot-reload
///   generations for the same current file content.
#[test]
fn source_reload_adapter_collapses_duplicate_source_paths_in_batch() {
    let dir = source_reload_test_dir("duplicate_source_batch");
    let asset = dir.join("style.css");
    let source = dir.join("Main.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&source, "module app.\n\npub value(): Int ->\n    1.\n").expect("write source");

    let mut adapter = VmSourceReloadAdapter::new();
    let events = adapter
        .publish_changed_files(&[source.clone(), asset, source.clone(), source])
        .expect("duplicate source batch should publish once");

    assert_eq!(events.len(), 1);
    let (module, generation) = published_event(&events[0]).expect("publish event");
    assert_eq!(module, "app");
    assert_eq!(generation.as_u64(), 1);
    assert_eq!(adapter.event_snapshots().len(), 1);

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies missing Terlan source files report stable read diagnostics.
///
/// Inputs:
/// - One `.terl` path that does not exist.
///
/// Output:
/// - Stable read error text and no code-server events.
///
/// Transformation:
/// - Covers filesystem failure before compile/publish so a watcher cannot
///   create a generation from an unreadable source event.
#[test]
fn source_reload_adapter_reports_unreadable_terlan_source() {
    let dir = source_reload_test_dir("missing_source");
    let path = dir.join("missing.terl");

    let mut adapter = VmSourceReloadAdapter::new();
    let error = adapter
        .publish_changed_file(&path)
        .expect_err("missing source should fail");

    assert!(
        error.contains("failed to read changed Terlan source"),
        "unexpected error: {error}"
    );
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies invalid changed source does not publish a generation.
///
/// Inputs:
/// - One malformed `.terl` file.
///
/// Output:
/// - Stable error text and no code-server events.
///
/// Transformation:
/// - Proves source reload failure is atomic at the adapter boundary: invalid
///   source cannot create an active or retiring VM generation.
#[test]
fn source_reload_adapter_rejects_invalid_source_without_publication() {
    let dir = source_reload_test_dir("invalid_source");
    let path = dir.join("broken.terl");
    fs::write(&path, "module broken.\n\npub value(: Int ->\n    1.\n")
        .expect("write broken source");

    let mut adapter = VmSourceReloadAdapter::new();
    let error = adapter
        .publish_changed_file(&path)
        .expect_err("invalid source should fail");

    assert!(
        error.contains("source hot reload compile failed"),
        "unexpected error: {error}"
    );
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies different source paths cannot publish the same module identity.
#[test]
fn source_reload_adapter_rejects_duplicate_modules_without_partial_publication() {
    let dir = source_reload_test_dir("duplicate_module_batch");
    let first = dir.join("First.terl");
    let second = dir.join("Second.terl");
    fs::write(
        &first,
        "module duplicate_module.\n\npub first(): Int ->\n    1.\n",
    )
    .expect("write first duplicate module");
    fs::write(
        &second,
        "module duplicate_module.\n\npub second(): Int ->\n    2.\n",
    )
    .expect("write second duplicate module");

    let mut adapter = VmSourceReloadAdapter::new();
    let error = adapter
        .publish_changed_files(&[first, second])
        .expect_err("duplicate module identity must reject watcher batch");

    assert_eq!(
        error,
        "error[vm.code_server.duplicate_staged_module]: batch contains duplicate module `duplicate_module`"
    );
    assert!(adapter.event_snapshots().is_empty());

    fs::remove_dir_all(dir).expect("remove source reload test dir");
}

/// Verifies duplicate metadata fails before a native image is admitted.
#[test]
fn native_reload_rejects_duplicate_modules_before_image_admission() {
    let artifact = |revision| VmStagedModuleArtifact {
        module: "duplicate_native".to_string(),
        artifact: VmModuleArtifact::new(
            format!("duplicate-native-{revision}"),
            format!("duplicate-native-map-{revision}"),
        ),
    };
    let mut adapter = VmSourceReloadAdapter::new();
    let missing_image = source_reload_test_dir("duplicate_native_batch").join("missing.so");

    let error = adapter
        .publish_native_generation(vec![artifact(1), artifact(2)], &missing_image, 0, 10)
        .expect_err("duplicate metadata must fail before native image loading");

    assert_eq!(
        error,
        "error[vm.code_server.duplicate_staged_module]: batch contains duplicate module `duplicate_native`"
    );
    assert!(adapter.event_snapshots().is_empty());
    assert_eq!(
        adapter.call_native("value", &[]).unwrap_err(),
        "error[vm.reload.native_generation]: no native generation is admitted"
    );

    fs::remove_dir_all(
        missing_image
            .parent()
            .expect("temporary image path has parent"),
    )
    .expect("remove native reload test dir");
}
