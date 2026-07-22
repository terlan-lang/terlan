use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{VmFatalDiagnosticBundle, VmFatalDiagnosticPolicy};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler::{VmScheduler, VmSchedulerConfig};

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("parity.Dump", function, 0)
        .with_source_path(format!("/private/host/{function}.terl"))
}

fn temp_directory(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "terlan-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ))
}

/// Replaces scheduler-signal, EXITING, and FREE ERTS dump inspection with one
/// bounded VM snapshot of every retained state plus missing observed identity.
#[test]
fn dump_suite_captures_loaded_exited_and_missing_subjects_in_one_stable_bundle() {
    let mut processes = VmProcessTable::default();
    let runnable = processes.spawn_root(source("loaded"));
    let blocked = processes.spawn_root(source("blocked"));
    let exited = processes.spawn_root(source("exiting"));
    processes.get_mut(blocked).expect("blocked process").block();
    processes
        .get_mut(runnable)
        .expect("loaded process")
        .add_resource_handle("secret-host-handle");
    processes
        .exit_process(
            exited,
            VmExitReason::Error("secret failure payload".to_string()),
        )
        .expect("retain terminal process");

    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(8, 4));
    scheduler
        .enqueue_runnable(&processes, runnable)
        .expect("queue loaded process");
    let missing = VmProcessId::from_raw_for_test(9_999);
    let bundle = VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(8, 32 * 1024).expect("enabled policy"),
        17,
        "vm.abort",
        &processes,
        &scheduler,
        &[exited, missing, missing],
    )
    .expect("capture fatal diagnostics")
    .expect("enabled capture");

    assert_eq!(bundle.schema, "terlan.vm.fatal-diagnostic.v2");
    assert_eq!(bundle.generation, 17);
    assert_eq!(bundle.scheduler.queued_processes, [runnable.as_u64()]);
    assert_eq!(bundle.processes.len(), 3);
    assert_eq!(bundle.processes[0].state, "runnable");
    assert_eq!(bundle.processes[0].resource_handle_count, 1);
    assert_eq!(bundle.processes[1].state, "blocked");
    assert_eq!(bundle.processes[2].state, "exited");
    assert_eq!(bundle.processes[2].resume_state, None);
    assert_eq!(bundle.processes[2].exit_kind, Some("error"));
    assert_eq!(bundle.missing_processes, [missing.as_u64()]);
    assert!(bundle.complete);

    let rendered = String::from_utf8(bundle.serialized_bytes().expect("serialize bundle"))
        .expect("fatal diagnostic is UTF-8 JSON");
    assert!(!rendered.contains("secret failure payload"));
    assert!(!rendered.contains("secret-host-handle"));
    assert!(!rendered.contains("/private/host"));
    assert!(rendered.contains("\"complete\": true"));

    let repeated = VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(8, 32 * 1024).expect("repeated policy"),
        17,
        "vm.abort",
        &processes,
        &scheduler,
        &[missing],
    )
    .expect("repeat capture")
    .expect("enabled repeated capture");
    assert_eq!(bundle, repeated);
}

/// Replaces Erlang heart environment behavior with explicit policy and atomic,
/// bounded, fail-closed support-bundle publication.
#[test]
fn dump_suite_policy_and_atomic_publication_are_fail_closed() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("publisher"));
    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, actor)
        .expect("queue publisher");

    assert_eq!(
        VmFatalDiagnosticBundle::capture(
            VmFatalDiagnosticPolicy::Disabled,
            0,
            "INVALID CAUSE IS IGNORED WHEN DISABLED",
            &processes,
            &scheduler,
            &[],
        )
        .expect("disabled capture"),
        None
    );
    assert!(VmFatalDiagnosticPolicy::enabled(0, 1).is_err());
    assert!(VmFatalDiagnosticPolicy::enabled(1, 0).is_err());
    assert!(VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(1, 32 * 1024).expect("subject policy"),
        3,
        "invalid cause",
        &processes,
        &scheduler,
        &[],
    )
    .is_err());
    assert!(VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(1, 32 * 1024).expect("missing subject policy"),
        3,
        "vm.abort",
        &processes,
        &scheduler,
        &[VmProcessId::from_raw_for_test(9_999)],
    )
    .is_err());
    assert!(VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(1, 16).expect("byte policy"),
        3,
        "vm.abort",
        &processes,
        &scheduler,
        &[],
    )
    .is_err());

    let bundle = VmFatalDiagnosticBundle::capture(
        VmFatalDiagnosticPolicy::enabled(1, 32 * 1024).expect("publish policy"),
        3,
        "vm.abort",
        &processes,
        &scheduler,
        &[],
    )
    .expect("capture publication")
    .expect("enabled publication");
    let directory = temp_directory("dump-suite-publication");
    let destination = directory.join("fatal-diagnostic.json");
    bundle
        .publish_atomic(&destination)
        .expect("publish complete bundle");
    let parsed: serde_json::Value =
        serde_json::from_slice(&fs::read(&destination).expect("read published fatal diagnostic"))
            .expect("published bundle is complete JSON");
    assert_eq!(parsed["schema"], "terlan.vm.fatal-diagnostic.v2");
    assert_eq!(parsed["complete"], true);
    assert_eq!(parsed["generation"], 3);
    let partials = fs::read_dir(&directory)
        .expect("read publication directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".partial"))
        .count();
    assert_eq!(partials, 0);

    let original = fs::read(&destination).expect("retain original bundle");
    assert!(bundle.publish_atomic(&destination).is_err());
    assert_eq!(
        fs::read(&destination).expect("existing bundle remains intact"),
        original
    );
    fs::remove_dir_all(directory).expect("remove publication fixture");
}
