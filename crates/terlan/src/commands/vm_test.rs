use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat, DocFormat};

/// Verifies the experimental Rust VM can AOT-compile and execute hello world.
///
/// Inputs:
/// - Temporary Terlan source importing `std.io.Console.println`.
///
/// Output:
/// - Captured VM output and `Unit` return value.
///
/// Transformation:
/// - Compiles through the normal frontend, builds a native image, and executes
///   the image's `main/0` export without a runtime IR fallback.
#[test]
fn vm_run_loads_hello_world_source_and_executes_main() {
    let root = unique_temp_dir("terlan-vm-hello");
    let source = root.join("Main.terl");
    fs::create_dir_all(&root).expect("create temp dir");
    fs::write(
        &source,
        "module vm_hello.Main.\n\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"hello from Rust VM\").\n",
    )
    .expect("write source");
    let mut lines = Vec::new();
    let mut output = |line: &str| lines.push(line.to_string());

    let value =
        run_source_file_in_vm(&source, "main", &test_state(), &mut output).expect("run in VM");

    assert_eq!(value, ReplValue::Unit);
    assert_eq!(lines, vec!["hello from Rust VM".to_string()]);
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies the Rust VM reports a missing native-image export loudly.
///
/// Inputs:
/// - Temporary source with `main/0`.
/// - Explicit missing entry function name.
///
/// Output:
/// - Stable AOT error naming the missing native export.
///
/// Transformation:
/// - Proves source execution cannot fall back to runtime IR interpretation.
#[test]
fn vm_run_reports_missing_entrypoint_as_vm_error() {
    let root = unique_temp_dir("terlan-vm-missing-entry");
    let source = root.join("Main.terl");
    fs::create_dir_all(&root).expect("create temp dir");
    fs::write(
        &source,
        "module vm_hello.Main.\n\npub main(): Unit ->\n    Unit.\n",
    )
    .expect("write source");
    let mut output = |_line: &str| {};

    let error =
        run_source_file_in_vm(&source, "missing", &test_state(), &mut output).expect_err("error");

    assert!(
        error.contains("error[vm.aot_export_missing]"),
        "unexpected VM error: {error}"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

#[test]
fn vm_run_rejects_unwired_capability_instead_of_spinning() {
    let root = unique_temp_dir("terlan-vm-unwired-capability");
    let source = root.join("Main.terl");
    fs::create_dir_all(&root).expect("create temp dir");
    fs::write(
        &source,
        "module vm_capability.Main.\n\npub main(): Bool ->\n    std.io.File.exists(\"missing.txt\").\n",
    )
    .expect("write source");
    let mut output = |_line: &str| {};

    let error = run_source_file_in_vm(&source, "main", &test_state(), &mut output)
        .expect_err("unsupported command capability");

    assert!(
        error.contains("error[vm.command_capability_unsupported]"),
        "unexpected VM error: {error}"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies VM reload publishes source files through the code server adapter.
///
/// Inputs:
/// - Two source files declaring the same module with different function bodies.
///
/// Output:
/// - Initial publish event followed by a hot-reload event.
///
/// Transformation:
/// - Exercises the experimental `terlc vm reload` command helper without a
///   long-running watcher so source reload has a concrete command boundary.
#[test]
fn vm_reload_publishes_changed_sources_through_code_server() {
    let root = unique_temp_dir("terlan-vm-reload");
    fs::create_dir_all(&root).expect("create temp dir");
    let first = root.join("First.terl");
    let second = root.join("Second.terl");
    fs::write(&first, "module app.\n\npub value(): Int ->\n    1.\n").expect("write first source");
    fs::write(&second, "module app.\n\npub value(): Int ->\n    2.\n")
        .expect("write second source");

    let events = reload_source_files_in_vm(&[first, second]).expect("reload sources");

    assert_eq!(events.len(), 2);
    assert_eq!(
        render_reload_event(&events[0]),
        "published app generation 1"
    );
    assert_eq!(
        render_reload_event(&events[1]),
        "hot-reloaded app generation 1 -> 2"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

#[test]
fn vm_reload_renders_generation_purge_event_without_internal_debug_shape() {
    use crate::runtime::vm::code_server::{VmCodeServer, VmModuleArtifact};

    let mut code_server = VmCodeServer::default();
    code_server.publish("app", VmModuleArtifact::new("a1", "source-map-a1"));
    code_server.publish("app", VmModuleArtifact::new("a2", "source-map-a2"));

    let events = code_server
        .purge_retired_generations("app")
        .expect("retired generation should purge");

    assert_eq!(events.len(), 1);
    assert_eq!(render_reload_event(&events[0]), "purged app generation 1");
}

/// Verifies VM reload rejects calls without Terlan source files.
///
/// Inputs:
/// - One non-source asset path.
///
/// Output:
/// - Stable command diagnostic.
///
/// Transformation:
/// - Keeps source reload separate from asset reload so the command cannot
///   report success without publishing a VM module generation.
#[test]
fn vm_reload_rejects_non_source_inputs() {
    let root = unique_temp_dir("terlan-vm-reload-asset");
    fs::create_dir_all(&root).expect("create temp dir");
    let asset = root.join("style.css");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");

    let error = reload_source_files_in_vm(&[asset]).expect_err("asset should not publish");

    assert_eq!(
        error,
        "terlc vm reload did not receive any .terl source files"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies VM reload accepts watcher-style mixed source and asset batches.
///
/// Inputs:
/// - One asset path and one Terlan source path.
///
/// Output:
/// - One source publication event and no error for the asset path.
///
/// Transformation:
/// - Proves the command helper reuses the VM source-reload batch boundary that
///   future dev/watch integrations can call without treating asset reloads as
///   VM source generations.
#[test]
fn vm_reload_ignores_assets_in_mixed_source_batch() {
    let root = unique_temp_dir("terlan-vm-reload-mixed");
    fs::create_dir_all(&root).expect("create temp dir");
    let asset = root.join("style.css");
    let source = root.join("Main.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&source, "module app.\n\npub value(): Int ->\n    1.\n").expect("write source");

    let events = reload_source_files_in_vm(&[asset, source]).expect("mixed reload batch");

    assert_eq!(events.len(), 1);
    assert_eq!(
        render_reload_event(&events[0]),
        "published app generation 1"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies VM reload diagnostics are opt-in and keep source arguments intact.
///
/// Inputs:
/// - Reload arguments containing one diagnostics flag and two source paths.
///
/// Output:
/// - Parsed reload command with diagnostics enabled and only real paths in the
///   source list.
///
/// Transformation:
/// - Keeps command parsing predictable for future watcher/dev-server callers
///   that may pass asset and source paths in the same batch.
#[test]
fn vm_reload_parses_diagnostics_flag_as_command_option() {
    let args = vec![
        "reload".to_string(),
        "--diagnostics".to_string(),
        "First.terl".to_string(),
        "Second.terl".to_string(),
    ];

    let parsed = parse_vm_args(&args);

    match parsed {
        VmArgs::Reload {
            sources,
            diagnostics,
        } => {
            assert!(diagnostics);
            assert_eq!(
                sources,
                vec![PathBuf::from("First.terl"), PathBuf::from("Second.terl")]
            );
        }
        _ => panic!("expected reload args"),
    }
}

/// Verifies VM reload can render an inspectable mixed-batch diagnostic summary.
///
/// Inputs:
/// - One asset path, one source path, and a duplicate source path.
///
/// Output:
/// - One publication event plus stable batch diagnostic counters.
///
/// Transformation:
/// - Exercises the command-facing report helper that backs
///   `terlc --experimental vm reload --diagnostics`.
#[test]
fn vm_reload_reports_mixed_batch_diagnostics() {
    let root = unique_temp_dir("terlan-vm-reload-diagnostics");
    fs::create_dir_all(&root).expect("create temp dir");
    let asset = root.join("style.css");
    let source = root.join("Main.terl");
    fs::write(&asset, "body { color: black; }\n").expect("write asset");
    fs::write(&source, "module app.\n\npub value(): Int ->\n    1.\n").expect("write source");

    let report =
        reload_source_files_in_vm_with_report(&[asset, source.clone(), source]).expect("report");

    assert_eq!(report.changed_paths, 3);
    assert_eq!(report.unique_source_paths, 1);
    assert_eq!(report.ignored_paths, 1);
    assert_eq!(report.duplicate_source_paths, 1);
    assert_eq!(report.events.len(), 1);
    assert_eq!(
        render_reload_diagnostics(&report),
        "reload diagnostics: changed_paths=3 unique_sources=1 ignored_paths=1 duplicate_sources=1 events=1"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies source reload replaces executable native generations, not metadata only.
#[test]
fn vm_native_reload_executes_two_compiled_generations() {
    let root = unique_temp_dir("terlan-vm-native-reload");
    fs::create_dir_all(&root).expect("create temp dir");
    let source = root.join("Counter.terl");
    let mut state = test_state();
    state.out_dir = root.join("build");
    let mut reload = VmNativeSourceReloadService::new();

    fs::write(
        &source,
        "module reload.Counter.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("write first generation");
    let first = reload
        .reload(std::slice::from_ref(&source), &state)
        .expect("admit first generation");
    assert_eq!(first.native_generation, 1);
    assert_eq!(first.replay.runtime_generation, 1);
    assert_eq!(first.replay.retained_events, 1);
    assert!(first.replay.replayable);
    assert_eq!(
        reload.call("reload.Counter.value", &[]),
        Ok(ReplValue::Int(1))
    );

    fs::write(
        &source,
        "module reload.Counter.\n\npub value(): Int ->\n    2.\n",
    )
    .expect("write second generation");
    let second = reload
        .reload(std::slice::from_ref(&source), &state)
        .expect("replace native generation");
    assert_eq!(second.native_generation, 2);
    assert_eq!(second.replay.runtime_generation, 2);
    assert_eq!(second.replay.retained_events, 2);
    assert_eq!(
        second.replay.schedulers[0]
            .events
            .iter()
            .map(|event| event.context.shard_epoch)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert!(matches!(
        second.sources.events.as_slice(),
        [VmCodeServerEvent::HotReloaded { .. }]
    ));
    assert_eq!(
        reload.call("reload.Counter.value", &[]),
        Ok(ReplValue::Int(2))
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Proves hot reload ignores non-source runtime artifacts and removes stale output.
#[test]
fn vm_native_reload_ignores_renamed_json_and_cleans_legacy_sidecars() {
    let root = unique_temp_dir("terlan-vm-native-reload-legacy");
    fs::create_dir_all(&root).expect("create temp dir");
    let source = root.join("Counter.terl");
    let renamed_json = root.join("serialized.tvm");
    let input_sidecar = root.join("serialized.tvm.json");
    let mut state = test_state();
    state.out_dir = root.join("build");
    let vm_dir = state.out_dir.join("vm-reload-aot/vm");
    let stale_image = vm_dir.join("stale.tvm");
    let output_sidecar = vm_dir.join("generation.tvm.json");
    let reuse_sidecar = vm_dir.join("generation.tvm.reuse");
    fs::create_dir_all(&vm_dir).expect("create seeded reload output");
    fs::write(
        &source,
        "module reload.Counter.\n\npub value(): Int ->\n    23.\n",
    )
    .expect("write source");
    for path in [&renamed_json, &input_sidecar, &stale_image, &output_sidecar] {
        fs::write(
            path,
            br#"{"vm_ir":{"functions":[{"instructions":["interpret-me"]}]}}"#,
        )
        .expect("write serialized runtime fixture");
    }
    fs::write(&reuse_sidecar, b"legacy reuse marker").expect("write reuse sidecar");
    let mut reload = VmNativeSourceReloadService::new();

    let report = reload
        .reload(
            &[renamed_json.clone(), input_sidecar.clone(), source],
            &state,
        )
        .expect("compile native reload generation");

    assert_eq!(report.sources.changed_paths, 3);
    assert_eq!(report.sources.unique_source_paths, 1);
    assert_eq!(report.sources.ignored_paths, 2);
    assert_eq!(report.sources.duplicate_source_paths, 0);
    assert_eq!(
        reload.call("reload.Counter.value", &[]),
        Ok(ReplValue::Int(23))
    );
    assert!(renamed_json.is_file());
    assert!(input_sidecar.is_file());
    assert!(!stale_image.exists());
    assert!(!output_sidecar.exists());
    assert!(!reuse_sidecar.exists());
    assert_eq!(
        fs::read_dir(&vm_dir)
            .expect("read reload output")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|value| value == "tvm"))
            .count(),
        1,
        "hot reload must publish one native image"
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Proves runtime execution is detached from the caller-controlled image path.
#[test]
fn admitted_native_generation_survives_source_image_replacement() {
    let root = unique_temp_dir("terlan-vm-sealed-generation");
    fs::create_dir_all(&root).expect("create temp dir");
    let source = root.join("Sealed.terl");
    let mut state = test_state();
    state.out_dir = root.join("build");
    let mut reload = VmNativeSourceReloadService::new();
    fs::write(
        &source,
        "module reload.Sealed.\n\npub value(): Int ->\n    41.\n",
    )
    .expect("write sealed generation");
    let publication = reload
        .reload(std::slice::from_ref(&source), &state)
        .expect("admit sealed generation");

    fs::write(&publication.native_image, b"{}\n").expect("replace caller image path");
    assert_eq!(
        reload.call("reload.Sealed.value", &[]),
        Ok(ReplValue::Int(41))
    );
    fs::remove_dir_all(root).expect("clean temp dir");
}

/// Verifies a timed-out pinned generation is quarantined without publication.
#[test]
fn vm_native_reload_quarantines_timed_out_generation_without_force_unload() {
    use crate::runtime::vm::pure_native::VmNativeGenerationReferenceClass;

    let root = unique_temp_dir("terlan-vm-native-reload-quarantine");
    fs::create_dir_all(&root).expect("create temp dir");
    let source = root.join("Pinned.terl");
    let mut state = test_state();
    state.out_dir = root.join("build");
    let mut reload = VmNativeSourceReloadService::new();
    fs::write(
        &source,
        "module reload.Pinned.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("write first generation");
    reload
        .reload(std::slice::from_ref(&source), &state)
        .expect("admit first generation");
    reload
        .pin_generation(VmNativeGenerationReferenceClass::Debugger)
        .expect("pin debugger metadata");
    fs::write(
        &source,
        "module reload.Pinned.\n\npub value(): Int ->\n    2.\n",
    )
    .expect("write candidate generation");

    let error = reload
        .reload_at(std::slice::from_ref(&source), &state, 50, 50)
        .expect_err("pinned generation must time out");

    assert!(error.contains("error[execution_shard.generation_quarantined]"));
    assert!(error.contains("debugger_pins=1"));
    assert!(reload
        .call("reload.Pinned.value", &[])
        .expect_err("quarantined shard rejects routing")
        .contains("found Quarantined"));
    let replay = reload
        .replay_evidence()
        .expect("quarantined generation replay evidence");
    assert_eq!(replay.retained_events, 1);
    assert_eq!(replay.schedulers[0].events[0].context.shard_epoch, Some(1));
    fs::remove_dir_all(root).expect("clean temp dir");
}

fn test_state() -> CliState {
    CliState {
        no_emit: false,
        incremental: false,
        timings: false,
        experimental: true,
        out_dir: PathBuf::from("_build"),
        cache_dir: None,
        trace_invalidation: false,
        diagnostic_format: DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        doc_format: DocFormat::Html,
        native_policy: NativePolicy::NativeBoundaryOptional,
        target_profile: TargetProfile::CoreV0,
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}
