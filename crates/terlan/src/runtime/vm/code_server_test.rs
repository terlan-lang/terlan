use super::{
    VmCodeBinding, VmCodeServer, VmCodeServerEvent, VmModuleArtifact, VmModuleGenerationId,
    VmModuleGenerationState,
};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn artifact(checksum: &str) -> VmModuleArtifact {
    VmModuleArtifact::new(checksum, format!("source-map-{checksum}"))
}

fn published_event(event: &VmCodeServerEvent) -> Option<(String, VmModuleGenerationId)> {
    match event {
        VmCodeServerEvent::Published { module, generation } => Some((module.clone(), *generation)),
        _ => None,
    }
}

fn hot_reloaded_event(
    event: &VmCodeServerEvent,
) -> Option<(
    String,
    VmModuleGenerationId,
    VmModuleGenerationState,
    VmModuleGenerationId,
)> {
    match event {
        VmCodeServerEvent::HotReloaded {
            module,
            previous_generation,
            previous_state,
            active_generation,
        } => Some((
            module.clone(),
            *previous_generation,
            *previous_state,
            *active_generation,
        )),
        _ => None,
    }
}

fn compiled_source_artifact(source_name: &str, source: &str) -> (String, VmModuleArtifact) {
    VmCodeServer::compile_source_artifact(source_name, source)
        .expect("source should compile through formal VM profile")
}

#[test]
fn code_server_event_extractors_classify_publish_and_reload_shapes() {
    let mut code_server = VmCodeServer::default();
    let published = code_server.publish("app.Main", artifact("a1"));
    let reloaded = code_server.publish("app.Main", artifact("a2"));

    assert!(published_event(&published).is_some());
    assert!(published_event(&reloaded).is_none());
    assert!(hot_reloaded_event(&published).is_none());
    assert!(hot_reloaded_event(&reloaded).is_some());
}

/// Verifies source hot-reload compile failures return stable VM diagnostics.
///
/// Inputs:
/// - Malformed Terlan source text and a source-facing file name.
///
/// Output:
/// - Error text that preserves the source name and VM hot-reload context.
///
/// Transformation:
/// - Exercises the formal pipeline error mapping used by `publish_source`
///   without publishing a generation.
#[test]
fn source_hot_reload_reports_compile_errors_for_invalid_source() {
    let error = VmCodeServer::compile_source_artifact("broken.terl", "module broken\n")
        .expect_err("malformed source should fail");

    assert!(error.contains("source hot reload compile failed for `broken.terl`"));
}

#[test]
fn code_server_failed_lifecycle_operations_are_mutation_free() {
    let mut code_server = VmCodeServer::default();
    let (_, generation) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("initial generation should publish");
    let snapshots_before = code_server.snapshots();
    let events_before = code_server.event_snapshots();

    code_server
        .publish_source("broken.terl", "module broken\n")
        .expect_err("invalid source publication should fail");
    code_server
        .purge_retired_generations("missing.Module")
        .expect_err("missing module purge should fail");
    code_server
        .promote_generation("app.Main", generation, &artifact("wrong"))
        .expect_err("artifact mismatch should reject promotion");
    code_server
        .release_process(&VmCodeBinding {
            pid: VmProcessId::from_raw_for_test(99),
            module: "app.Main".to_string(),
            generation,
        })
        .expect_err("unbound process release should fail");

    assert_eq!(code_server.snapshots(), snapshots_before);
    assert_eq!(code_server.event_snapshots(), events_before);
    assert_eq!(
        code_server
            .active_generation("app.Main")
            .expect("failed operations must preserve the active generation"),
        generation
    );
}

#[test]
fn code_server_publishes_initial_generation_and_exposes_snapshot() {
    let mut code_server = VmCodeServer::default();

    let event = code_server.publish("app.Main", artifact("a1"));

    let (module, generation) = published_event(&event).expect("expected initial publish event");
    assert_eq!(module, "app.Main");
    assert_eq!(generation.as_u64(), 1);
    assert_eq!(
        code_server
            .active_generation("app.Main")
            .expect("active generation should exist"),
        generation
    );

    let snapshots = code_server.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].module, "app.Main");
    assert_eq!(snapshots[0].generation, generation);
    assert_eq!(snapshots[0].state, VmModuleGenerationState::Active);
    assert_eq!(snapshots[0].active_processes, 0);
    assert_eq!(snapshots[0].checksum, "a1");
    assert_eq!(snapshots[0].source_map_id, "source-map-a1");
}

#[test]
fn code_server_hot_reload_binds_new_processes_to_new_generation() {
    let mut processes = VmProcessTable::default();
    let old_process = processes.spawn_root(source("old"));
    let new_process = processes.spawn_root(source("new"));
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("expected initial publish");
    let old_binding = code_server
        .bind_process_to_active(&processes, old_process, "app.Main")
        .expect("old process should bind");
    assert_eq!(old_binding.generation, generation_1);

    let event = code_server.publish("app.Main", artifact("a2"));

    let (_, previous_generation, previous_state, active_generation) =
        hot_reloaded_event(&event).expect("expected hot reload event");
    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retiring);
    assert_eq!(active_generation.as_u64(), 2);

    let new_binding = code_server
        .bind_process_to_active(&processes, new_process, "app.Main")
        .expect("new process should bind");
    assert_eq!(new_binding.generation, active_generation);
    assert_eq!(old_binding.generation, generation_1);

    let snapshots = code_server.snapshots();
    let old_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.generation == generation_1)
        .expect("old generation snapshot should exist");
    let new_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.generation == active_generation)
        .expect("new generation snapshot should exist");
    assert_eq!(old_snapshot.state, VmModuleGenerationState::Retiring);
    assert_eq!(old_snapshot.active_processes, 1);
    assert_eq!(new_snapshot.state, VmModuleGenerationState::Active);
    assert_eq!(new_snapshot.active_processes, 1);
}

#[test]
fn code_server_release_retires_drained_old_generation() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("worker"));
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("expected initial publish");
    let binding = code_server
        .bind_process_to_active(&processes, pid, "app.Main")
        .expect("process should bind");
    code_server.publish("app.Main", artifact("a2"));

    let event = code_server
        .release_process(&binding)
        .expect("release should succeed");

    assert_eq!(
        event,
        Some(VmCodeServerEvent::GenerationRetired {
            module: "app.Main".to_string(),
            generation: generation_1
        })
    );
    let old_snapshot = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == generation_1)
        .expect("old generation snapshot should exist");
    assert_eq!(old_snapshot.state, VmModuleGenerationState::Retired);
    assert_eq!(old_snapshot.active_processes, 0);
}

#[test]
fn code_server_hot_reload_retires_unused_previous_generation_immediately() {
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("expected initial publish");

    let event = code_server.publish("app.Main", artifact("a2"));

    assert_eq!(
        event,
        VmCodeServerEvent::HotReloaded {
            module: "app.Main".to_string(),
            previous_generation: generation_1,
            previous_state: VmModuleGenerationState::Retired,
            active_generation: code_server
                .active_generation("app.Main")
                .expect("active generation should exist")
        }
    );
    let old_snapshot = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == generation_1)
        .expect("old generation snapshot should exist");
    assert_eq!(old_snapshot.state, VmModuleGenerationState::Retired);
}

#[test]
fn code_server_purges_retired_generations_in_generation_order() {
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("initial generation should publish");
    let (_, _, _, generation_2) =
        hot_reloaded_event(&code_server.publish("app.Main", artifact("a2")))
            .expect("second generation should reload");
    let (_, _, _, generation_3) =
        hot_reloaded_event(&code_server.publish("app.Main", artifact("a3")))
            .expect("third generation should reload");

    let events = code_server
        .purge_retired_generations("app.Main")
        .expect("retired generations should purge");

    assert_eq!(
        events,
        vec![
            VmCodeServerEvent::GenerationPurged {
                module: "app.Main".to_string(),
                generation: generation_1,
            },
            VmCodeServerEvent::GenerationPurged {
                module: "app.Main".to_string(),
                generation: generation_2,
            },
        ]
    );
    assert_eq!(
        code_server.snapshots(),
        vec![super::VmModuleGenerationSnapshot {
            module: "app.Main".to_string(),
            generation: generation_3,
            state: VmModuleGenerationState::Active,
            active_processes: 0,
            checksum: "a3".to_string(),
            source_map_id: "source-map-a3".to_string(),
        }]
    );
    let event_snapshots = code_server.event_snapshots();
    assert_eq!(event_snapshots.len(), 5);
    assert_eq!(event_snapshots[3].sequence, 4);
    assert_eq!(event_snapshots[3].event, events[0]);
    assert_eq!(event_snapshots[4].sequence, 5);
    assert_eq!(event_snapshots[4].event, events[1]);
}

#[test]
fn code_server_reload_after_purge_keeps_generation_identity_monotonic() {
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("initial generation should publish");
    let (_, previous_generation, previous_state, generation_2) =
        hot_reloaded_event(&code_server.publish("app.Main", artifact("a2")))
            .expect("second generation should reload");
    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retired);

    assert_eq!(
        code_server
            .purge_retired_generations("app.Main")
            .expect("first generation should purge"),
        vec![VmCodeServerEvent::GenerationPurged {
            module: "app.Main".to_string(),
            generation: generation_1,
        }]
    );

    let event = code_server.publish("app.Main", artifact("a3"));
    let (_, previous_generation, previous_state, generation_3) =
        hot_reloaded_event(&event).expect("publish after purge should remain a reload");
    assert_eq!(previous_generation, generation_2);
    assert_eq!(previous_state, VmModuleGenerationState::Retired);
    assert!(generation_1.as_u64() < generation_2.as_u64());
    assert!(generation_2.as_u64() < generation_3.as_u64());

    let snapshots = code_server.snapshots();
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots
        .iter()
        .all(|snapshot| snapshot.generation != generation_1));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.generation == generation_2
            && snapshot.state == VmModuleGenerationState::Retired
            && snapshot.checksum == "a2"
    }));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.generation == generation_3
            && snapshot.state == VmModuleGenerationState::Active
            && snapshot.checksum == "a3"
    }));

    let events = code_server.event_snapshots();
    assert_eq!(events.len(), 4);
    assert_eq!(events[3].sequence, 4);
    assert_eq!(events[3].event, event);
}

#[test]
fn code_server_purge_preserves_process_bound_retiring_generation_until_release() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("worker"));
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("initial generation should publish");
    let binding = code_server
        .bind_process_to_active(&processes, pid, "app.Main")
        .expect("process should bind");
    code_server.publish("app.Main", artifact("a2"));

    assert!(code_server
        .purge_retired_generations("app.Main")
        .expect("retiring generation should be retained")
        .is_empty());
    assert!(code_server
        .snapshots()
        .iter()
        .any(|snapshot| snapshot.generation == generation_1
            && snapshot.state == VmModuleGenerationState::Retiring
            && snapshot.active_processes == 1));

    assert_eq!(
        code_server
            .release_process(&binding)
            .expect("last process should release"),
        Some(VmCodeServerEvent::GenerationRetired {
            module: "app.Main".to_string(),
            generation: generation_1,
        })
    );
    assert_eq!(
        code_server
            .purge_retired_generations("app.Main")
            .expect("drained generation should purge"),
        vec![VmCodeServerEvent::GenerationPurged {
            module: "app.Main".to_string(),
            generation: generation_1,
        }]
    );
    assert!(code_server
        .snapshots()
        .iter()
        .all(|snapshot| snapshot.generation != generation_1));
}

#[test]
fn code_server_process_bound_reload_records_ordered_retire_and_purge_events() {
    let mut processes = VmProcessTable::default();
    let old_process = processes.spawn_root(source("old-generation"));
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish("app.Main", artifact("a1")))
        .expect("initial generation should publish");
    let binding = code_server
        .bind_process_to_active(&processes, old_process, "app.Main")
        .expect("old process should bind");
    let reload = code_server.publish("app.Main", artifact("a2"));
    let (_, previous_generation, previous_state, generation_2) =
        hot_reloaded_event(&reload).expect("second generation should reload");
    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retiring);

    assert!(code_server
        .purge_retired_generations("app.Main")
        .expect("process-bound generation should be retained")
        .is_empty());
    assert_eq!(code_server.event_snapshots().len(), 2);

    let retired = code_server
        .release_process(&binding)
        .expect("final binding should release")
        .expect("drained generation should retire");
    let purged = code_server
        .purge_retired_generations("app.Main")
        .expect("retired generation should purge");
    assert_eq!(
        purged,
        vec![VmCodeServerEvent::GenerationPurged {
            module: "app.Main".to_string(),
            generation: generation_1,
        }]
    );

    let events = code_server.event_snapshots();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].event, reload);
    assert_eq!(events[2].sequence, 3);
    assert_eq!(events[2].event, retired);
    assert_eq!(events[3].sequence, 4);
    assert_eq!(events[3].event, purged[0]);
    assert_eq!(
        code_server
            .active_generation("app.Main")
            .expect("new generation should remain active"),
        generation_2
    );
}

#[test]
fn code_server_purge_rejects_missing_module_and_keeps_active_generation() {
    let mut code_server = VmCodeServer::default();
    assert_eq!(
        code_server
            .purge_retired_generations("missing.Module")
            .expect_err("missing module should fail"),
        "module `missing.Module` has no generations"
    );

    let published = code_server.publish("app.Main", artifact("a1"));
    let snapshots_before = code_server.snapshots();
    let events_before = code_server.event_snapshots();
    assert!(code_server
        .purge_retired_generations("app.Main")
        .expect("active-only module should be a no-op")
        .is_empty());
    assert_eq!(code_server.snapshots(), snapshots_before);
    assert_eq!(code_server.event_snapshots(), events_before);
    assert_eq!(events_before[0].event, published);
}

#[test]
fn code_server_reports_missing_module_and_exited_process_diagnostics() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("worker"));
    let mut code_server = VmCodeServer::default();

    assert_eq!(
        code_server
            .bind_process_to_active(&processes, pid, "missing.Module")
            .expect_err("missing module should fail"),
        "module `missing.Module` has no active generation"
    );

    code_server.publish("app.Main", artifact("a1"));
    processes
        .exit_process(pid, VmExitReason::Killed)
        .expect("process exit should succeed");
    assert_eq!(
        code_server
            .bind_process_to_active(&processes, pid, "app.Main")
            .expect_err("exited process should not bind"),
        format!("process {} has exited", pid.as_u64())
    );
}

#[test]
fn code_server_reports_missing_active_generation_and_missing_process() {
    let processes = VmProcessTable::default();
    let missing = VmProcessId::from_raw_for_test(99);
    let mut code_server = VmCodeServer::default();

    assert_eq!(
        code_server
            .active_generation("missing.Module")
            .expect_err("missing active generation should fail"),
        "module `missing.Module` has no active generation"
    );

    code_server.publish("app.Main", artifact("a1"));
    assert_eq!(
        code_server
            .bind_process_to_active(&processes, missing, "app.Main")
            .expect_err("missing process should not bind"),
        "missing process 99"
    );
}

#[test]
fn code_server_reports_stale_release_binding_and_active_release_noop() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("worker"));
    let mut code_server = VmCodeServer::default();
    code_server.publish("app.Main", artifact("a1"));
    let binding = code_server
        .bind_process_to_active(&processes, pid, "app.Main")
        .expect("process should bind");

    assert_eq!(
        code_server
            .release_process(&binding)
            .expect("active release should succeed"),
        None
    );
    let snapshot = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == binding.generation)
        .expect("active generation snapshot should exist");
    assert_eq!(snapshot.state, VmModuleGenerationState::Active);
    assert_eq!(snapshot.active_processes, 0);

    let stale = VmCodeBinding {
        pid,
        module: "missing.Module".to_string(),
        generation: binding.generation,
    };
    assert_eq!(
        code_server
            .release_process(&stale)
            .expect_err("stale binding should fail"),
        format!(
            "module `missing.Module` has no generation {}",
            binding.generation.as_u64()
        )
    );
}

#[test]
fn code_server_rejects_duplicate_release_and_keeps_unique_process_bindings() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("worker"));
    let mut code_server = VmCodeServer::default();
    code_server.publish("app.Main", artifact("a1"));
    let first_binding = code_server
        .bind_process_to_active(&processes, pid, "app.Main")
        .expect("first process bind should succeed");
    let second_binding = code_server
        .bind_process_to_active(&processes, pid, "app.Main")
        .expect("duplicate process bind should stay idempotent");

    let before_release = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == first_binding.generation)
        .expect("active generation snapshot should exist");
    assert_eq!(second_binding, first_binding);
    assert_eq!(before_release.active_processes, 1);
    assert_eq!(
        code_server
            .release_process(&first_binding)
            .expect("first release should succeed"),
        None
    );

    assert_eq!(
        code_server
            .release_process(&second_binding)
            .expect_err("duplicate release should fail"),
        format!(
            "process {} is not bound to generation {} for module `app.Main`",
            pid.as_u64(),
            first_binding.generation.as_u64()
        )
    );
    let after_release = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == first_binding.generation)
        .expect("active generation snapshot should remain available");
    assert_eq!(after_release.active_processes, 0);
}

#[test]
fn source_hot_reload_publishes_compiled_generations_and_preserves_bindings() {
    let source_v1 = "module app.Main.\n\npub value(): Int ->\n    1.\n";
    let source_v2 = "module app.Main.\n\npub value(): Int ->\n    2.\n";
    let (module, artifact_v1) = compiled_source_artifact("src/app/Main.terl", source_v1);
    let (module_v2, artifact_v2) = compiled_source_artifact("src/app/Main.terl", source_v2);
    assert_eq!(module, module_v2);
    assert_ne!(artifact_v1.checksum, artifact_v2.checksum);

    let mut processes = VmProcessTable::default();
    let old_process = processes.spawn_root(source("old-source-call"));
    let new_process = processes.spawn_root(source("new-source-call"));
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) =
        published_event(&code_server.publish(module.clone(), artifact_v1.clone()))
            .expect("expected initial source publish");
    let old_binding = code_server
        .bind_process_to_active(&processes, old_process, &module)
        .expect("old process should bind to first source generation");

    let event = code_server.publish(module.clone(), artifact_v2.clone());

    let (_, previous_generation, previous_state, active_generation) =
        hot_reloaded_event(&event).expect("expected source hot reload event");
    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retiring);
    assert_eq!(old_binding.generation, generation_1);

    let new_binding = code_server
        .bind_process_to_active(&processes, new_process, &module)
        .expect("new process should bind to newest source generation");
    assert_eq!(new_binding.generation, active_generation);

    let snapshots = code_server.snapshots();
    let old_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.generation == generation_1)
        .expect("old source generation should remain inspectable");
    let new_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.generation == active_generation)
        .expect("new source generation should be inspectable");
    assert_eq!(old_snapshot.state, VmModuleGenerationState::Retiring);
    assert_eq!(old_snapshot.active_processes, 1);
    assert_eq!(old_snapshot.checksum, artifact_v1.checksum);
    assert_eq!(new_snapshot.state, VmModuleGenerationState::Active);
    assert_eq!(new_snapshot.active_processes, 1);
    assert_eq!(new_snapshot.source_map_id, artifact_v2.source_map_id);
}

#[test]
fn source_hot_reload_publish_source_compiles_and_publishes_new_generation() {
    let source_v1 = "module app.Main.\n\npub value(): Int ->\n    1.\n";
    let source_v2 = "module app.Main.\n\npub value(): Int ->\n    2.\n";
    let mut code_server = VmCodeServer::default();

    let published = code_server
        .publish_source("src/app/Main.terl", source_v1)
        .expect("initial source publish should compile");
    let (module, generation_1) =
        published_event(&published).expect("expected initial source publish");

    let reloaded = code_server
        .publish_source("src/app/Main.terl", source_v2)
        .expect("updated source publish should compile");

    let (reloaded_module, previous_generation, previous_state, active_generation) =
        hot_reloaded_event(&reloaded).expect("expected source hot reload");
    assert_eq!(module, "app.Main");
    assert_eq!(reloaded_module, module);
    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retired);
    assert_ne!(active_generation, generation_1);

    let snapshots = code_server.snapshots();
    let old_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.generation == generation_1)
        .expect("old generation should remain inspectable");
    let new_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.generation == active_generation)
        .expect("new generation should remain inspectable");
    assert!(old_snapshot.checksum.starts_with("source-fnv1a64:"));
    assert!(new_snapshot.checksum.starts_with("source-fnv1a64:"));
    assert_ne!(old_snapshot.checksum, new_snapshot.checksum);
    assert!(new_snapshot.source_map_id.contains("src/app/Main.terl"));
}

#[test]
fn source_hot_reload_detects_changed_helper_function_body() {
    let source_v1 = concat!(
        "module app.Main.\n\n",
        "pub value(): Int ->\n",
        "    helper().\n\n",
        "helper(): Int ->\n",
        "    1.\n"
    );
    let source_v2 = concat!(
        "module app.Main.\n\n",
        "pub value(): Int ->\n",
        "    helper().\n\n",
        "helper(): Int ->\n",
        "    2.\n"
    );
    let (module, artifact_v1) = compiled_source_artifact("src/app/Main.terl", source_v1);
    let (module_v2, artifact_v2) = compiled_source_artifact("src/app/Main.terl", source_v2);
    assert_eq!(module, module_v2);
    assert_ne!(
        artifact_v1.checksum, artifact_v2.checksum,
        "helper body edits must produce a new source generation identity"
    );

    let mut code_server = VmCodeServer::default();
    let (_, generation_1) = published_event(&code_server.publish(module.clone(), artifact_v1))
        .expect("expected initial helper-backed publish");
    let (_, previous_generation, previous_state, active_generation) =
        hot_reloaded_event(&code_server.publish(module.clone(), artifact_v2.clone()))
            .expect("expected helper-body hot reload");

    assert_eq!(previous_generation, generation_1);
    assert_eq!(previous_state, VmModuleGenerationState::Retired);
    assert_ne!(active_generation, generation_1);
    let active_snapshot = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == active_generation)
        .expect("new helper-backed generation should be inspectable");
    assert_eq!(active_snapshot.checksum, artifact_v2.checksum);
    assert_eq!(active_snapshot.state, VmModuleGenerationState::Active);
}

#[test]
fn source_hot_reload_rollback_validates_artifact_metadata() {
    let source_v1 = "module app.Main.\n\npub value(): Int ->\n    1.\n";
    let source_v2 = "module app.Main.\n\npub value(): Int ->\n    2.\n";
    let (module, artifact_v1) = compiled_source_artifact("src/app/Main.terl", source_v1);
    let (_, artifact_v2) = compiled_source_artifact("src/app/Main.terl", source_v2);
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) =
        published_event(&code_server.publish(module.clone(), artifact_v1.clone()))
            .expect("expected initial source publish");
    let (_, _, _, generation_2) =
        hot_reloaded_event(&code_server.publish(module.clone(), artifact_v2))
            .expect("expected source hot reload");

    let error = code_server
        .promote_generation(&module, generation_1, &artifact("wrong"))
        .expect_err("rollback must reject mismatched artifact metadata");
    assert!(error.contains("checksum `"));
    assert!(error.contains("source map `"));
    assert!(error.contains("expected checksum `wrong`"));

    let event = code_server
        .promote_generation(&module, generation_1, &artifact_v1)
        .expect("valid source rollback should promote older generation");

    assert_eq!(
        event,
        VmCodeServerEvent::HotReloaded {
            module: module.clone(),
            previous_generation: generation_2,
            previous_state: VmModuleGenerationState::Retired,
            active_generation: generation_1,
        }
    );
    assert_eq!(
        code_server
            .active_generation(&module)
            .expect("rolled back generation should be active"),
        generation_1
    );
    let snapshots = code_server.snapshots();
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.generation == generation_1)
            .expect("rolled back generation should have snapshot")
            .state,
        VmModuleGenerationState::Active
    );
    assert_eq!(
        snapshots
            .iter()
            .find(|snapshot| snapshot.generation == generation_2)
            .expect("replaced generation should have snapshot")
            .state,
        VmModuleGenerationState::Retired
    );
}

#[test]
fn source_hot_reload_rollback_keeps_live_replaced_generation_retiring() {
    let source_v1 = "module app.Main.\n\npub value(): Int ->\n    1.\n";
    let source_v2 = "module app.Main.\n\npub value(): Int ->\n    2.\n";
    let (module, artifact_v1) = compiled_source_artifact("src/app/Main.terl", source_v1);
    let (_, artifact_v2) = compiled_source_artifact("src/app/Main.terl", source_v2);
    let mut processes = VmProcessTable::default();
    let active_process = processes.spawn_root(source("active-new-generation"));
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) =
        published_event(&code_server.publish(module.clone(), artifact_v1.clone()))
            .expect("expected initial source publish");
    let (_, _, _, generation_2) =
        hot_reloaded_event(&code_server.publish(module.clone(), artifact_v2))
            .expect("expected source hot reload");
    let active_binding = code_server
        .bind_process_to_active(&processes, active_process, &module)
        .expect("process should bind to generation being replaced");

    let event = code_server
        .promote_generation(&module, generation_1, &artifact_v1)
        .expect("rollback should preserve live replaced generation");

    assert_eq!(
        event,
        VmCodeServerEvent::HotReloaded {
            module: module.clone(),
            previous_generation: generation_2,
            previous_state: VmModuleGenerationState::Retiring,
            active_generation: generation_1,
        }
    );
    let replaced = code_server
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.generation == generation_2)
        .expect("replaced generation should remain inspectable");
    assert_eq!(replaced.state, VmModuleGenerationState::Retiring);
    assert_eq!(replaced.active_processes, 1);

    assert_eq!(
        code_server
            .release_process(&active_binding)
            .expect("release should drain replaced generation"),
        Some(VmCodeServerEvent::GenerationRetired {
            module,
            generation: generation_2,
        })
    );
}

#[test]
fn source_hot_reload_reports_missing_generation_and_active_promote_noop() {
    let source_v1 = "module app.Main.\n\npub value(): Int ->\n    1.\n";
    let (module, artifact_v1) = compiled_source_artifact("src/app/Main.terl", source_v1);
    let mut code_server = VmCodeServer::default();

    let missing_module_error = code_server
        .promote_generation(&module, VmModuleGenerationId(1), &artifact_v1)
        .expect_err("missing module should fail");
    assert_eq!(
        missing_module_error,
        "module `app.Main` has no generation 1"
    );

    let (_, generation_1) =
        published_event(&code_server.publish(module.clone(), artifact_v1.clone()))
            .expect("expected initial source publish");
    let (_, other_generation) =
        published_event(&code_server.publish("other.Main", artifact("other")))
            .expect("expected independent source publish");
    let missing_generation_error = code_server
        .promote_generation(&module, other_generation, &artifact_v1)
        .expect_err("wrong-module generation should fail");
    assert_eq!(
        missing_generation_error,
        format!(
            "module `app.Main` has no generation {}",
            other_generation.as_u64()
        )
    );

    assert_eq!(
        code_server
            .promote_generation(&module, generation_1, &artifact_v1)
            .expect("promoting active generation is a stable no-op event"),
        VmCodeServerEvent::HotReloaded {
            module,
            previous_generation: generation_1,
            previous_state: VmModuleGenerationState::Active,
            active_generation: generation_1,
        }
    );
}

/// Verifies reload and rollback events remain available for inspection.
///
/// Inputs:
/// - Two source-backed generations for the same module.
/// - A valid rollback promotion to the first generation.
///
/// Output:
/// - Ordered event snapshots recording publish, hot reload, and rollback.
///
/// Transformation:
/// - Locks runtime inspection as a persistent VM code-server feature instead
///   of relying on one-shot return values from mutation calls.
#[test]
fn source_hot_reload_records_reload_and_rollback_events_for_inspection() {
    let source_v1 = "module app.Main.\n\npub value(): Int ->\n    1.\n";
    let source_v2 = "module app.Main.\n\npub value(): Int ->\n    2.\n";
    let (module, artifact_v1) = compiled_source_artifact("src/app/Main.terl", source_v1);
    let (_, artifact_v2) = compiled_source_artifact("src/app/Main.terl", source_v2);
    let mut code_server = VmCodeServer::default();
    let (_, generation_1) =
        published_event(&code_server.publish(module.clone(), artifact_v1.clone()))
            .expect("expected initial source publish");
    let (_, _, _, generation_2) =
        hot_reloaded_event(&code_server.publish(module.clone(), artifact_v2))
            .expect("expected source hot reload");

    code_server
        .promote_generation(&module, generation_1, &artifact_v1)
        .expect("valid rollback should record event");

    let events = code_server.event_snapshots();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(
        events[0].event,
        VmCodeServerEvent::Published {
            module: module.clone(),
            generation: generation_1,
        }
    );
    assert_eq!(events[1].sequence, 2);
    assert_eq!(
        events[1].event,
        VmCodeServerEvent::HotReloaded {
            module: module.clone(),
            previous_generation: generation_1,
            previous_state: VmModuleGenerationState::Retired,
            active_generation: generation_2,
        }
    );
    assert_eq!(events[2].sequence, 3);
    assert_eq!(
        events[2].event,
        VmCodeServerEvent::HotReloaded {
            module,
            previous_generation: generation_2,
            previous_state: VmModuleGenerationState::Retired,
            active_generation: generation_1,
        }
    );
}
