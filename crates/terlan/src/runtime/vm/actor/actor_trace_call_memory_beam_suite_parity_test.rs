use super::super::actor_heap_limit::VmActorHeapLimitPolicy;
use super::super::{VmActorReceive, VmActorRuntime, VmExitReason, VmProcessId, VmProcessSource};
use crate::runtime::vm::call_count::VmCallCountState;
use crate::runtime::vm::call_memory::{VmCallMemoryProcessSnapshot, VmCallMemoryState};
use crate::runtime::vm::ReplValue;

fn function(name: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceCallMemory", name, arity)
}

fn receive_accounted(runtime: &mut VmActorRuntime, pid: VmProcessId) -> u64 {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(pid)
        .expect("receive attributed message")
    else {
        panic!("attributed process must have a message")
    };
    u64::try_from(message.accounted_bytes).expect("logical message bytes fit u64")
}

#[test]
fn trace_call_memory_suite_basic_late_nested_and_counter_isolation_contract() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(function("first", 0));
    let second = runtime.spawn_root(function("second", 0));
    let alloc = function("alloc_2tuple", 0);
    runtime.enable_function_call_memory(alloc.clone());

    for (pid, bytes) in [(first, 24), (second, 8), (first, 24)] {
        let outcome = runtime
            .reserve_actor_heap(pid, bytes, VmActorHeapLimitPolicy::Reject)
            .expect("reserve attributed heap");
        assert_eq!(outcome.pressure.requested_bytes, bytes);
        assert!(runtime
            .record_function_allocations(
                &alloc,
                pid,
                1,
                u64::try_from(bytes).expect("test bytes fit u64"),
            )
            .expect("record accepted heap allocation"));
    }
    assert_eq!(
        runtime.function_call_memory_state(&alloc),
        VmCallMemoryState::Enabled {
            processes: vec![
                VmCallMemoryProcessSnapshot {
                    pid: first.as_u64(),
                    calls: 2,
                    allocated_bytes: 48,
                },
                VmCallMemoryProcessSnapshot {
                    pid: second.as_u64(),
                    calls: 1,
                    allocated_bytes: 8,
                },
            ],
        }
    );

    let late = function("late_trace", 0);
    assert!(!runtime
        .record_function_allocations(&late, first, 1, 99)
        .expect("disabled attribution is a no-op"));
    runtime.enable_function_call_memory(late.clone());
    assert!(runtime
        .record_function_allocations(&late, first, 1, 12)
        .expect("record only post-enable allocation"));
    assert_eq!(
        runtime.function_call_memory_state(&late),
        VmCallMemoryState::Enabled {
            processes: vec![VmCallMemoryProcessSnapshot {
                pid: first.as_u64(),
                calls: 1,
                allocated_bytes: 12,
            }],
        }
    );

    let upper = function("upper", 0);
    let middle = function("middle", 1);
    let lower = function("lower", 1);
    runtime.enable_function_call_memory(upper.clone());
    runtime.enable_function_call_memory(lower.clone());
    assert!(runtime
        .record_function_allocations(&upper, first, 1, 12)
        .expect("record upper-owned allocations"));
    assert!(!runtime
        .record_function_allocations(&middle, first, 1, 3)
        .expect("unprofiled middle allocation remains outside its own row"));
    assert!(runtime
        .record_function_allocations(&lower, first, 1, 16)
        .expect("record lower-owned allocations"));
    assert_eq!(
        runtime.function_call_memory_state(&upper),
        VmCallMemoryState::Enabled {
            processes: vec![VmCallMemoryProcessSnapshot {
                pid: first.as_u64(),
                calls: 1,
                allocated_bytes: 12,
            }],
        }
    );
    assert_eq!(
        runtime.function_call_memory_state(&lower),
        VmCallMemoryState::Enabled {
            processes: vec![VmCallMemoryProcessSnapshot {
                pid: first.as_u64(),
                calls: 1,
                allocated_bytes: 16,
            }],
        }
    );

    runtime.enable_function_call_count(alloc.clone());
    runtime
        .record_function_entries(&alloc, 1)
        .expect("record independent call count");
    let memory_before = runtime.function_call_memory_state(&alloc);
    assert!(runtime.disable_function_call_count(&alloc));
    assert_eq!(
        runtime.function_call_count_state(&alloc),
        VmCallCountState::Disabled
    );
    assert!(runtime
        .record_function_allocations(&alloc, first, 1, 3)
        .expect("memory attribution survives call-count disable"));
    assert_ne!(runtime.function_call_memory_state(&alloc), memory_before);

    let snapshots = runtime.function_call_memory_snapshots();
    assert_eq!(runtime.function_call_memory_snapshots(), snapshots);
    assert!(snapshots.windows(2).all(|rows| {
        (rows[0].source.function.as_str(), rows[0].source.arity)
            <= (rows[1].source.function.as_str(), rows[1].source.arity)
    }));
}

#[test]
fn trace_call_memory_suite_receive_spawn_parallel_restart_and_width_contract() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(function("sender", 0));
    let receiver = runtime.spawn_root(function("receiver", 0));
    let receive = function("receive_message", 0);

    runtime
        .send(sender, receiver, ReplValue::Int(1))
        .expect("pre-enable send");
    receive_accounted(&mut runtime, receiver);
    runtime.enable_function_call_memory(receive.clone());
    let mut received_bytes = 0;
    for payload in [ReplValue::Int(2), ReplValue::String("owned".to_string())] {
        runtime
            .send(sender, receiver, payload)
            .expect("profiled send");
        let bytes = receive_accounted(&mut runtime, receiver);
        received_bytes += bytes;
        assert!(runtime
            .record_function_allocations(&receive, receiver, 1, bytes)
            .expect("attribute receive allocation"));
    }
    assert_eq!(
        runtime.function_call_memory_state(&receive),
        VmCallMemoryState::Enabled {
            processes: vec![VmCallMemoryProcessSnapshot {
                pid: receiver.as_u64(),
                calls: 2,
                allocated_bytes: received_bytes,
            }],
        }
    );
    assert_eq!(
        runtime
            .memory_metrics(receiver)
            .expect("receive memory metrics")
            .current_bytes,
        0,
        "heap release must not erase cumulative function attribution"
    );

    let workers = (0..3)
        .map(|index| runtime.spawn_root(function(&format!("worker-{index}"), 0)))
        .collect::<Vec<_>>();
    for worker in &workers {
        assert!(runtime
            .record_function_allocations(&receive, *worker, 1, 3)
            .expect("record isolated worker allocation"));
    }
    runtime
        .exit_actor(workers[0], VmExitReason::Normal)
        .expect("exit profiled worker");
    let VmCallMemoryState::Enabled { processes } = runtime.function_call_memory_state(&receive)
    else {
        panic!("receive profile must remain enabled")
    };
    assert_eq!(processes.len(), 4);
    assert!(processes
        .iter()
        .any(|row| row.pid == workers[0].as_u64() && row.allocated_bytes == 3));

    let spawn = function("spawn_memory", 1);
    runtime.enable_function_call_memory(spawn.clone());
    let child = runtime
        .spawn_child(sender, function("spawned", 1))
        .expect("spawn profiled child");
    runtime
        .reserve_actor_heap(child, 34, VmActorHeapLimitPolicy::Reject)
        .expect("reserve child argument heap");
    runtime
        .record_function_allocations(&spawn, child, 1, 34)
        .expect("attribute spawned child heap");
    runtime
        .exit_actor(child, VmExitReason::Normal)
        .expect("exit spawned child");
    assert_eq!(
        runtime.function_call_memory_state(&spawn),
        VmCallMemoryState::Enabled {
            processes: vec![VmCallMemoryProcessSnapshot {
                pid: child.as_u64(),
                calls: 1,
                allocated_bytes: 34,
            }],
        }
    );

    runtime
        .restart_function_call_memory(&spawn)
        .expect("restart spawned allocation profile");
    assert_eq!(
        runtime.function_call_memory_state(&spawn),
        VmCallMemoryState::Enabled {
            processes: Vec::new(),
        }
    );
    let wide = function("wide_allocations", 2);
    runtime.enable_function_call_memory(wide.clone());
    assert!(runtime
        .record_function_allocations(&wide, sender, u64::MAX, u64::MAX)
        .expect("record full-width allocation counters"));
    assert_eq!(
        runtime
            .record_function_allocations(&wide, sender, 1, 0)
            .expect_err("call overflow must fail atomically"),
        format!(
            "VM call memory call overflow for parity.TraceCallMemory.wide_allocations/2 process {} at {}",
            sender.as_u64(),
            u64::MAX
        )
    );
    assert_eq!(
        runtime.function_call_memory_state(&wide),
        VmCallMemoryState::Enabled {
            processes: vec![VmCallMemoryProcessSnapshot {
                pid: sender.as_u64(),
                calls: u64::MAX,
                allocated_bytes: u64::MAX,
            }],
        }
    );
    assert!(runtime.disable_function_call_memory(&wide));
    assert_eq!(
        runtime.function_call_memory_state(&wide),
        VmCallMemoryState::Disabled
    );
}
