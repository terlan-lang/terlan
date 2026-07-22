use super::super::code_server::{VmCodeBinding, VmCodeServer, VmCodeServerEvent, VmModuleArtifact};
use super::super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable};
use super::super::ReplValue;

const MODULE: &str = "code.versions";

fn process_source(function: &str) -> VmProcessSource {
    VmProcessSource::new(MODULE, function, 0)
}

fn artifact(version: u64) -> VmModuleArtifact {
    VmModuleArtifact::new(
        format!("version-{version}"),
        format!("code.versions:{version}"),
    )
}

fn request_version(
    processes: &mut VmProcessTable,
    code_server: &VmCodeServer,
    requester: VmProcessId,
    worker: VmProcessId,
    binding: &VmCodeBinding,
) -> u64 {
    processes
        .send(requester, worker, ReplValue::Atom("version".to_string()))
        .expect("version request should send");
    let request = processes
        .get_mut(worker)
        .expect("worker process")
        .receive_next()
        .expect("version request should arrive");
    assert_eq!(request.sender, requester);
    assert_eq!(request.payload, ReplValue::Atom("version".to_string()));

    let generation = code_server
        .snapshot_for_binding(binding)
        .expect("worker binding should remain inspectable")
        .generation
        .as_u64();
    let generation_value = i64::try_from(generation).expect("test generation should fit Int");
    let payload = ReplValue::Tuple(vec![
        ReplValue::Atom("version".to_string()),
        ReplValue::Int(generation_value),
    ]);
    processes
        .send(worker, requester, payload.clone())
        .expect("version reply should send");
    let reply = processes
        .get_mut(requester)
        .expect("requester process")
        .receive_next()
        .expect("version reply should arrive");
    assert_eq!(reply.sender, worker);
    assert_eq!(reply.payload, payload);
    generation
}

/// Replaces OTP's version loop with mailbox-visible VM generation identity.
#[test]
fn process_reports_its_bound_generation_across_module_reload() {
    let mut processes = VmProcessTable::default();
    let requester = processes.spawn_root(process_source("requester"));
    let old_worker = processes.spawn_root(process_source("loop"));
    let mut code_server = VmCodeServer::default();
    code_server.publish(MODULE, artifact(1));
    let old_binding = code_server
        .bind_process_to_active(&processes, old_worker, MODULE)
        .expect("old worker should bind");

    assert_eq!(
        request_version(
            &mut processes,
            &code_server,
            requester,
            old_worker,
            &old_binding,
        ),
        1
    );

    code_server.publish(MODULE, artifact(2));
    let new_worker = processes.spawn_root(process_source("loop"));
    let new_binding = code_server
        .bind_process_to_active(&processes, new_worker, MODULE)
        .expect("new worker should bind");

    assert_eq!(
        request_version(
            &mut processes,
            &code_server,
            requester,
            old_worker,
            &old_binding,
        ),
        1
    );
    assert_eq!(
        request_version(
            &mut processes,
            &code_server,
            requester,
            new_worker,
            &new_binding,
        ),
        2
    );

    processes
        .exit_process(old_worker, VmExitReason::Normal)
        .expect("old worker should exit");
    assert!(matches!(
        code_server
            .release_process(&old_binding)
            .expect("old generation binding should release"),
        Some(VmCodeServerEvent::GenerationRetired { generation, .. })
            if generation == old_binding.generation
    ));
}

/// Rejects forged and released bindings without changing code-server state.
#[test]
fn generation_inspection_rejects_unowned_bindings_without_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("loop"));
    let unbound = processes.spawn_root(process_source("unbound"));
    let mut code_server = VmCodeServer::default();
    code_server.publish(MODULE, artifact(1));
    let binding = code_server
        .bind_process_to_active(&processes, owner, MODULE)
        .expect("owner should bind");
    let forged = VmCodeBinding {
        pid: unbound,
        module: binding.module.clone(),
        generation: binding.generation,
    };
    let snapshots_before = code_server.snapshots();
    let events_before = code_server.event_snapshots();

    assert_eq!(
        code_server
            .snapshot_for_binding(&forged)
            .expect_err("unowned binding must fail"),
        format!(
            "process {} is not bound to generation 1 for module `{MODULE}`",
            unbound.as_u64()
        )
    );
    assert_eq!(code_server.snapshots(), snapshots_before);
    assert_eq!(code_server.event_snapshots(), events_before);

    code_server
        .release_process(&binding)
        .expect("owned binding should release");
    let snapshots_after_release = code_server.snapshots();
    let events_after_release = code_server.event_snapshots();
    assert_eq!(
        code_server
            .snapshot_for_binding(&binding)
            .expect_err("released binding must fail"),
        format!(
            "process {} is not bound to generation 1 for module `{MODULE}`",
            owner.as_u64()
        )
    );
    assert_eq!(code_server.snapshots(), snapshots_after_release);
    assert_eq!(code_server.event_snapshots(), events_after_release);
}
