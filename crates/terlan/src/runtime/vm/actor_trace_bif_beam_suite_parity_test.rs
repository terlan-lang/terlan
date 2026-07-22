use super::{VmActorRuntime, VmProcessSource};
use crate::runtime::vm::code_server::{VmCodeServer, VmCodeServerEvent, VmModuleGenerationState};
use crate::runtime::vm::process::{VmProcessLocation, VmProcessTable};
use crate::runtime::vm::scheduler::VmSchedulerDecision;

fn actor_source(function: &str) -> VmProcessSource {
    VmProcessSource::new("parity.TraceBif", function, 0)
}

fn primitive_source(module: &str, function: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new(module, function, arity)
}

#[test]
fn trace_bif_suite_call_return_timestamp_and_cursor_contract() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(actor_source("bif_process"));
    let cursor = runtime.system_profile_cursor();
    let mut observed_stacks = Vec::new();

    let first_run = runtime
        .run_next(|process, slice| {
            assert_eq!(slice.pid, actor);
            assert_eq!(slice.tick, 1);
            process.set_current_location(actor_source("bif_process"), 4);
            process
                .enter_execution_frame(primitive_source("std.time", "monotonic", 0), 0, 11)
                .expect("enter monotonic-time primitive");
            observed_stacks.push(process.current_stacktrace());
            assert_eq!(
                process
                    .pop_execution_frame()
                    .expect("return from monotonic-time primitive"),
                VmProcessLocation {
                    source: primitive_source("std.time", "monotonic", 0),
                    instruction_offset: 0,
                }
            );

            process
                .enter_execution_frame(primitive_source("std.runtime", "statistics", 1), 2, 19)
                .expect("enter statistics primitive");
            observed_stacks.push(process.current_stacktrace());
            assert_eq!(
                process
                    .pop_execution_frame()
                    .expect("return from statistics primitive"),
                VmProcessLocation {
                    source: primitive_source("std.runtime", "statistics", 1),
                    instruction_offset: 2,
                }
            );
            assert_eq!(
                process.current_location(),
                &VmProcessLocation {
                    source: actor_source("bif_process"),
                    instruction_offset: 19,
                }
            );
            VmSchedulerDecision::Yield { reductions: 4 }
        })
        .expect("run traced primitive calls");
    assert_eq!(first_run.pid, Some(actor));

    runtime
        .run_next(|_, slice| {
            assert_eq!(slice.pid, actor);
            assert_eq!(slice.tick, 2);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("finish traced actor slice");

    assert_eq!(observed_stacks.len(), 2);
    assert_eq!(
        observed_stacks[0],
        [
            VmProcessLocation {
                source: primitive_source("std.time", "monotonic", 0),
                instruction_offset: 0,
            },
            VmProcessLocation {
                source: actor_source("bif_process"),
                instruction_offset: 11,
            },
        ]
    );
    assert_eq!(
        observed_stacks[1],
        [
            VmProcessLocation {
                source: primitive_source("std.runtime", "statistics", 1),
                instruction_offset: 2,
            },
            VmProcessLocation {
                source: actor_source("bif_process"),
                instruction_offset: 19,
            },
        ]
    );

    let profile = runtime
        .system_profile_since(cursor)
        .expect("capture primitive-call scheduler profile");
    assert_eq!(
        profile
            .events
            .iter()
            .map(|event| event.transition)
            .collect::<Vec<_>>(),
        ["dequeue", "enqueue", "dequeue"]
    );
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].sequence + 1 == events[1].sequence));
    assert!(profile
        .events
        .windows(2)
        .all(|events| events[0].tick <= events[1].tick));
    assert!(profile.events.iter().all(|event| {
        event.pid == actor.as_u64()
            && event.location.source == actor_source("bif_process")
            && event.location.instruction_offset == 19
    }));
    assert_eq!(
        profile,
        runtime
            .system_profile_since(cursor)
            .expect("cursor replay must be immutable")
    );
    assert!(runtime
        .system_profile_since(profile.next_cursor)
        .expect("a delivered cursor excludes earlier activity")
        .events
        .is_empty());
}

#[test]
fn trace_bif_suite_retired_code_inspection_and_purge_contract() {
    const MODULE: &str = "trace_bif_primitive";
    let source_v1 = concat!(
        "module trace_bif_primitive.\n\n",
        "pub time(): Int -> 1.\n\n",
        "pub statistics(value: Int): Int -> value.\n",
    );
    let source_v2 = concat!(
        "module trace_bif_primitive.\n\n",
        "pub time(): Int -> 2.\n\n",
        "pub statistics(value: Int): Int -> value + 1.\n",
    );
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(actor_source("bif_process"));
    let mut code_server = VmCodeServer::default();

    code_server
        .publish_source("trace_bif_primitive.terl", source_v1)
        .expect("publish primitive generation");
    let old_binding = code_server
        .enter_process_function(&mut processes, actor, MODULE, "time", 0, 0, 7)
        .expect("enter old primitive generation");
    code_server
        .publish_source("trace_bif_primitive.terl", source_v2)
        .expect("publish replacement primitive generation");

    let while_running = code_server.snapshots_for_module(MODULE);
    assert_eq!(while_running.len(), 2);
    assert_eq!(while_running[0].generation, old_binding.generation);
    assert_eq!(while_running[0].state, VmModuleGenerationState::Retiring);
    assert_eq!(while_running[0].active_processes, 1);
    assert_eq!(while_running[1].state, VmModuleGenerationState::Active);
    assert_eq!(
        code_server.snapshots_for_module(MODULE),
        while_running,
        "inspection must not mutate retiring code"
    );

    let (returned, retired) = code_server
        .return_process_function(&mut processes, actor)
        .expect("return from old primitive generation");
    assert_eq!(returned.source, primitive_source(MODULE, "time", 0));
    assert_eq!(
        retired,
        Some(VmCodeServerEvent::GenerationRetired {
            module: MODULE.to_string(),
            generation: old_binding.generation,
        })
    );
    assert_eq!(
        code_server
            .snapshots_for_module(MODULE)
            .iter()
            .map(|snapshot| snapshot.state)
            .collect::<Vec<_>>(),
        [
            VmModuleGenerationState::Retired,
            VmModuleGenerationState::Active,
        ]
    );

    assert_eq!(
        code_server
            .purge_retired_generations(MODULE)
            .expect("purge drained primitive generation"),
        [VmCodeServerEvent::GenerationPurged {
            module: MODULE.to_string(),
            generation: old_binding.generation,
        }]
    );
    let after_purge = code_server.snapshots_for_module(MODULE);
    assert_eq!(after_purge.len(), 1);
    assert_eq!(after_purge[0].state, VmModuleGenerationState::Active);
    assert!(after_purge
        .iter()
        .all(|snapshot| snapshot.generation != old_binding.generation));
    assert_eq!(
        code_server
            .event_snapshots_for_module(MODULE)
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
}
