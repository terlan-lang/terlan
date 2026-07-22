use std::sync::{Arc, Barrier};

use super::{
    VmCodeServer, VmConcurrentCodeServer, VmConcurrentPublishOutcome, VmModuleArtifact,
    VmModuleGenerationState,
};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};

const MODULE: &str = "code.parallel_load_model";
const WORKERS: usize = 160;

fn artifact(revision: usize) -> VmModuleArtifact {
    VmModuleArtifact::new(
        format!("parallel-token-{revision}"),
        format!("parallel-source-map-{revision}"),
    )
}

#[test]
fn code_parallel_load_suite_simultaneous_identical_publish_happens_once() {
    let code_server = VmConcurrentCodeServer::default();
    let barrier = Arc::new(Barrier::new(WORKERS));
    let outcomes = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(WORKERS);
        for _ in 0..WORKERS {
            let code_server = code_server.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || {
                barrier.wait();
                code_server
                    .publish_if_changed(MODULE, artifact(1))
                    .expect("parallel publication should not fail")
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("publisher thread should finish"))
            .collect::<Vec<_>>()
    });

    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, VmConcurrentPublishOutcome::Published(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, VmConcurrentPublishOutcome::Reused { .. }))
            .count(),
        WORKERS - 1
    );
    assert!(outcomes
        .iter()
        .all(|outcome| outcome.generation().as_u64() == 1));

    let snapshots = code_server.snapshots().expect("concurrent snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].state, VmModuleGenerationState::Active);
    assert_eq!(snapshots[0].checksum, "parallel-token-1");
    assert_eq!(
        code_server
            .event_snapshots()
            .expect("concurrent event snapshots")
            .len(),
        1
    );
}

#[test]
fn code_parallel_load_suite_shard_local_workers_switch_without_global_lock() {
    let barrier = Arc::new(Barrier::new(WORKERS));
    let event_counts = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(WORKERS);
        for _ in 0..WORKERS {
            let barrier = Arc::clone(&barrier);
            handles.push(scope.spawn(move || {
                let mut code_server = VmCodeServer::default();
                let mut processes = VmProcessTable::default();
                let pid = processes.spawn_root(VmProcessSource::new(MODULE, "check", 1));
                code_server.publish(MODULE, artifact(1));
                let mut binding = code_server
                    .bind_process_to_active(&processes, pid, MODULE)
                    .expect("shard actor should bind to initial generation");
                barrier.wait();

                for revision in 2..=25 {
                    code_server.publish(MODULE, artifact(revision));
                    let (switched, retirement) = code_server
                        .switch_process_to_active(&processes, pid, MODULE)
                        .expect("shard actor should switch generations");
                    assert!(retirement.is_some());
                    binding = switched;
                    assert_eq!(
                        code_server
                            .purge_retired_generations(MODULE)
                            .expect("drained generation should purge")
                            .len(),
                        1
                    );
                    let snapshots = code_server.snapshots();
                    assert_eq!(snapshots.len(), 1);
                    assert_eq!(snapshots[0].generation, binding.generation);
                    assert_eq!(snapshots[0].active_processes, 1);
                    assert_eq!(snapshots[0].checksum, format!("parallel-token-{revision}"));
                }

                assert_eq!(
                    code_server
                        .release_process(&binding)
                        .expect("final shard actor release"),
                    None
                );
                code_server
                    .unload_active_generation(MODULE)
                    .expect("unbound active generation should unload");
                assert_eq!(
                    code_server
                        .purge_retired_generations(MODULE)
                        .expect("final generation should purge")
                        .len(),
                    1
                );
                assert!(code_server.snapshots().is_empty());
                let events = code_server.event_snapshots();
                assert!(events
                    .windows(2)
                    .all(|pair| pair[1].sequence == pair[0].sequence + 1));
                events.len()
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("shard worker should finish"))
            .collect::<Vec<_>>()
    });
    assert_eq!(event_counts, vec![75; WORKERS]);
}
