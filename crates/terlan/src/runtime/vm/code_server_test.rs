use super::{VmCodeServer, VmCodeServerEvent, VmModuleArtifact, VmModuleGenerationState};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn artifact(checksum: &str) -> VmModuleArtifact {
    VmModuleArtifact::new(checksum, format!("source-map-{checksum}"))
}

#[test]
fn code_server_publishes_initial_generation_and_exposes_snapshot() {
    let mut code_server = VmCodeServer::default();

    let event = code_server.publish("app.Main", artifact("a1"));

    let VmCodeServerEvent::Published { module, generation } = event else {
        panic!("expected initial publish event");
    };
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
    let VmCodeServerEvent::Published {
        generation: generation_1,
        ..
    } = code_server.publish("app.Main", artifact("a1"))
    else {
        panic!("expected initial publish");
    };
    let old_binding = code_server
        .bind_process_to_active(&processes, old_process, "app.Main")
        .expect("old process should bind");
    assert_eq!(old_binding.generation, generation_1);

    let event = code_server.publish("app.Main", artifact("a2"));

    let VmCodeServerEvent::HotReloaded {
        previous_generation,
        previous_state,
        active_generation,
        ..
    } = event
    else {
        panic!("expected hot reload event");
    };
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
    let VmCodeServerEvent::Published {
        generation: generation_1,
        ..
    } = code_server.publish("app.Main", artifact("a1"))
    else {
        panic!("expected initial publish");
    };
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
    let VmCodeServerEvent::Published {
        generation: generation_1,
        ..
    } = code_server.publish("app.Main", artifact("a1"))
    else {
        panic!("expected initial publish");
    };

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
