use super::{VmActorRuntime, VmExitReason, VmProcessSource};
use crate::runtime::vm::call_count::VmCallCountState;
use crate::runtime::vm::call_memory::VmCallMemoryState;
use crate::runtime::vm::call_time::{VmCallTimeMode, VmCallTimeProcessSnapshot, VmCallTimeState};

fn function(name: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceCallTime", name, arity)
}

#[test]
fn trace_call_time_suite_basic_lifecycle_and_exclusive_nested_contract() {
    let mut runtime = VmActorRuntime::default();
    let worker = runtime.spawn_root(function("worker", 0));
    let seq = function("seq", 3);
    let seq_r_entry = function("seq_r", 3);
    let seq_r_loop = function("seq_r", 4);

    assert!(!runtime
        .record_function_time(&seq, worker, 700, 2_800)
        .expect("disabled timing is a no-op"));
    runtime.enable_function_call_time(seq.clone());
    runtime.enable_function_call_time(seq_r_entry.clone());
    runtime.enable_function_call_time(seq_r_loop.clone());
    assert_eq!(
        runtime.function_call_time_state(&seq),
        VmCallTimeState::Active {
            processes: Vec::new(),
        }
    );

    runtime
        .record_function_time(&seq, worker, 700, 2_800)
        .expect("record iterative function time");
    runtime
        .record_function_time(&seq_r_entry, worker, 1, 2)
        .expect("record recursive entry time");
    runtime
        .record_function_time(&seq_r_loop, worker, 700, 2_100)
        .expect("record recursive loop time");
    assert_eq!(
        runtime.function_call_time_state(&seq),
        VmCallTimeState::Active {
            processes: vec![VmCallTimeProcessSnapshot {
                pid: worker.as_u64(),
                calls: 700,
                exclusive_ticks: 2_800,
            }],
        }
    );
    assert_eq!(
        runtime.function_call_time_state(&seq_r_entry),
        VmCallTimeState::Active {
            processes: vec![VmCallTimeProcessSnapshot {
                pid: worker.as_u64(),
                calls: 1,
                exclusive_ticks: 2,
            }],
        }
    );

    runtime.enable_function_call_time(seq.clone());
    assert_eq!(
        runtime.function_call_time_state(&seq),
        VmCallTimeState::Active {
            processes: vec![VmCallTimeProcessSnapshot {
                pid: worker.as_u64(),
                calls: 700,
                exclusive_ticks: 2_800,
            }],
        },
        "enable must preserve existing rows"
    );
    runtime
        .pause_function_call_time(&seq)
        .expect("pause enabled timing");
    assert!(!runtime
        .record_function_time(&seq, worker, 100, 400)
        .expect("paused timing is a no-op"));
    assert_eq!(
        runtime.function_call_time_state(&seq),
        VmCallTimeState::Paused {
            processes: vec![VmCallTimeProcessSnapshot {
                pid: worker.as_u64(),
                calls: 700,
                exclusive_ticks: 2_800,
            }],
        }
    );
    runtime
        .restart_function_call_time(&seq)
        .expect("restart enabled timing");
    assert_eq!(
        runtime.function_call_time_state(&seq),
        VmCallTimeState::Active {
            processes: Vec::new(),
        }
    );

    let outer = function("a_function", 1);
    let inner = function("a_called_function", 1);
    let leaf = function("dec", 1);
    for source in [&outer, &inner, &leaf] {
        runtime.enable_function_call_time(source.clone());
    }
    runtime
        .record_function_time(&outer, worker, 2_100, 2_100)
        .expect("attribute only outer exclusive ticks");
    runtime
        .record_function_time(&inner, worker, 2_100, 4_200)
        .expect("attribute only inner exclusive ticks");
    runtime
        .record_function_time(&leaf, worker, 2_100, 6_300)
        .expect("attribute leaf exclusive ticks");
    for (source, ticks) in [(&outer, 2_100), (&inner, 4_200), (&leaf, 6_300)] {
        let VmCallTimeState::Active { processes } = runtime.function_call_time_state(source) else {
            panic!("nested function timing must be active")
        };
        assert_eq!(processes[0].exclusive_ticks, ticks);
    }
    let outer_after_return = runtime.function_call_time_state(&outer);
    runtime
        .record_function_time(&seq_r_loop, worker, 1, 10_000)
        .expect("unrelated work after return");
    assert_eq!(runtime.function_call_time_state(&outer), outer_after_return);
}

#[test]
fn trace_call_time_suite_process_snapshot_isolation_and_profiler_independence_contract() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(function("first", 0));
    let second = runtime.spawn_root(function("second", 0));
    let timed = function("timed", 1);
    runtime.enable_function_call_time(timed.clone());

    for (pid, calls, ticks) in [(second, 5, 25), (first, 3, 12), (second, 2, 9)] {
        runtime
            .record_function_time(&timed, pid, calls, ticks)
            .expect("record isolated process timing");
    }
    runtime
        .exit_actor(first, VmExitReason::Normal)
        .expect("exit profiled process");
    assert_eq!(
        runtime.function_call_time_state(&timed),
        VmCallTimeState::Active {
            processes: vec![
                VmCallTimeProcessSnapshot {
                    pid: first.as_u64(),
                    calls: 3,
                    exclusive_ticks: 12,
                },
                VmCallTimeProcessSnapshot {
                    pid: second.as_u64(),
                    calls: 7,
                    exclusive_ticks: 34,
                },
            ],
        },
        "inspection is ordered and retains post-exit history"
    );

    runtime.enable_function_call_count(timed.clone());
    runtime.enable_function_call_memory(timed.clone());
    runtime
        .record_function_entries(&timed, 11)
        .expect("record independent call count");
    runtime
        .record_function_allocations(&timed, second, 7, 70)
        .expect("record independent call memory");
    let time_before = runtime.function_call_time_state(&timed);
    assert!(runtime.disable_function_call_count(&timed));
    assert_eq!(
        runtime.function_call_count_state(&timed),
        VmCallCountState::Disabled
    );
    assert!(runtime.disable_function_call_memory(&timed));
    assert_eq!(
        runtime.function_call_memory_state(&timed),
        VmCallMemoryState::Disabled
    );
    assert_eq!(runtime.function_call_time_state(&timed), time_before);

    let wide = function("wide", 2);
    runtime.enable_function_call_time(wide.clone());
    runtime
        .record_function_time(&wide, second, u64::MAX, u64::MAX)
        .expect("record full-width timing counters");
    assert_eq!(
        runtime
            .record_function_time(&wide, second, 1, 0)
            .expect_err("call overflow must reject atomically"),
        format!(
            "VM call time call overflow for parity.TraceCallTime.wide/2 process {} at {}",
            second.as_u64(),
            u64::MAX
        )
    );
    assert_eq!(
        runtime.function_call_time_state(&wide),
        VmCallTimeState::Active {
            processes: vec![VmCallTimeProcessSnapshot {
                pid: second.as_u64(),
                calls: u64::MAX,
                exclusive_ticks: u64::MAX,
            }],
        }
    );

    let snapshots = runtime.function_call_time_snapshots();
    assert_eq!(runtime.function_call_time_snapshots(), snapshots);
    assert!(snapshots.windows(2).all(|rows| {
        (
            &rows[0].source.module,
            &rows[0].source.function,
            rows[0].source.arity,
        ) <= (
            &rows[1].source.module,
            &rows[1].source.function,
            rows[1].source.arity,
        )
    }));
    assert_eq!(
        snapshots.last().expect("wide snapshot").mode,
        VmCallTimeMode::Active
    );
    assert!(runtime.disable_function_call_time(&wide));
    assert_eq!(
        runtime.function_call_time_state(&wide),
        VmCallTimeState::Disabled
    );
    assert_eq!(
        runtime
            .pause_function_call_time(&wide)
            .expect_err("cannot pause disabled timing"),
        "cannot pause disabled VM call time for parity.TraceCallTime.wide/2"
    );
}
