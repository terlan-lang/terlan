use super::{VmActorRuntime, VmProcessSource};
use crate::runtime::vm::call_count::{VmCallCountMode, VmCallCountState};
use crate::runtime::vm::process::VmProcessLocation;
use crate::runtime::vm::scheduler::VmSchedulerDecision;

fn function(name: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("parity.TraceCallCount", name, arity)
}

#[test]
fn trace_call_count_suite_recursive_arity_and_snapshot_contract() {
    let mut runtime = VmActorRuntime::default();
    let seq = function("seq", 3);
    let seq_r_entry = function("seq_r", 3);
    let seq_r_loop = function("seq_r", 4);

    runtime.enable_function_call_count(seq.clone());
    runtime.enable_function_call_count(seq_r_entry.clone());
    runtime.enable_function_call_count(seq_r_loop.clone());
    assert!(runtime
        .record_function_entries(&seq, 1_000)
        .expect("record stack-recursive entries"));
    assert_eq!(
        runtime.function_call_count_state(&seq),
        VmCallCountState::Active { count: 1_000 }
    );
    assert_eq!(
        runtime.function_call_count_state(&seq_r_entry),
        VmCallCountState::Active { count: 0 }
    );

    assert!(runtime
        .record_function_entries(&seq_r_entry, 1)
        .expect("record tail-recursive entry"));
    assert!(runtime
        .record_function_entries(&seq_r_loop, 1_000)
        .expect("record tail-recursive loop entries"));
    assert_eq!(
        runtime.function_call_count_state(&seq_r_entry),
        VmCallCountState::Active { count: 1 }
    );
    assert_eq!(
        runtime.function_call_count_state(&seq_r_loop),
        VmCallCountState::Active { count: 1_000 }
    );

    let snapshots = runtime.function_call_count_snapshots();
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| (
                snapshot.source.function.as_str(),
                snapshot.source.arity,
                snapshot.mode,
                snapshot.count,
            ))
            .collect::<Vec<_>>(),
        [
            ("seq", 3, VmCallCountMode::Active, 1_000),
            ("seq_r", 3, VmCallCountMode::Active, 1),
            ("seq_r", 4, VmCallCountMode::Active, 1_000),
        ]
    );
    assert_eq!(
        runtime.function_call_count_snapshots(),
        snapshots,
        "call-count inspection must be immutable"
    );
}

#[test]
fn trace_call_count_suite_pause_restart_disable_and_stack_contract() {
    let mut runtime = VmActorRuntime::default();
    let seq = function("seq", 3);
    assert_eq!(
        runtime
            .pause_function_call_count(&seq)
            .expect_err("disabled counter cannot pause"),
        "cannot pause disabled VM call count for parity.TraceCallCount.seq/3"
    );

    runtime.enable_function_call_count(seq.clone());
    assert!(runtime
        .record_function_entries(&seq, 100)
        .expect("record enabled entries"));
    runtime.enable_function_call_count(seq.clone());
    assert_eq!(
        runtime.function_call_count_state(&seq),
        VmCallCountState::Active { count: 100 },
        "enabling an active counter must not reset it"
    );

    runtime
        .pause_function_call_count(&seq)
        .expect("pause active counter");
    assert!(!runtime
        .record_function_entries(&seq, 100)
        .expect("paused recording is a no-op"));
    assert_eq!(
        runtime.function_call_count_state(&seq),
        VmCallCountState::Paused { count: 100 }
    );
    runtime
        .restart_function_call_count(&seq)
        .expect("restart paused counter");
    assert_eq!(
        runtime.function_call_count_state(&seq),
        VmCallCountState::Active { count: 0 }
    );
    assert!(runtime
        .record_function_entries(&seq, u64::MAX)
        .expect("record full-width count"));
    assert_eq!(
        runtime
            .record_function_entries(&seq, 1)
            .expect_err("overflow must fail"),
        format!(
            "VM call count overflow for parity.TraceCallCount.seq/3 at {}",
            u64::MAX
        )
    );
    assert_eq!(
        runtime.function_call_count_state(&seq),
        VmCallCountState::Active { count: u64::MAX },
        "overflow rejection must be atomic"
    );
    assert!(runtime.disable_function_call_count(&seq));
    assert!(!runtime.disable_function_call_count(&seq));
    assert_eq!(
        runtime.function_call_count_state(&seq),
        VmCallCountState::Disabled
    );
    assert!(!runtime
        .record_function_entries(&seq, 1)
        .expect("disabled recording is a no-op"));

    let seq_r_entry = function("seq_r", 3);
    let seq_r_loop = function("seq_r", 4);
    runtime.enable_function_call_count(seq_r_entry.clone());
    runtime.enable_function_call_count(seq_r_loop.clone());
    let actor = runtime.spawn_root(function("combo", 0));
    runtime
        .run_next(|process, _| {
            process
                .enter_execution_frame(seq_r_entry.clone(), 0, 5)
                .expect("enter counted function");
            process
                .enter_execution_frame(seq_r_loop.clone(), 3, 1)
                .expect("enter counted recursive helper");
            assert_eq!(
                process.current_stacktrace(),
                [
                    VmProcessLocation {
                        source: seq_r_loop.clone(),
                        instruction_offset: 3,
                    },
                    VmProcessLocation {
                        source: seq_r_entry.clone(),
                        instruction_offset: 1,
                    },
                    VmProcessLocation {
                        source: function("combo", 0),
                        instruction_offset: 5,
                    },
                ]
            );
            process
                .pop_execution_frame()
                .expect("return recursive helper");
            process.pop_execution_frame().expect("return entry helper");
            VmSchedulerDecision::Yield { reductions: 4 }
        })
        .expect("run counted function stack");
    assert!(runtime
        .record_function_entries(&seq_r_entry, 1)
        .expect("record outer call"));
    assert!(runtime
        .record_function_entries(&seq_r_loop, 3)
        .expect("record recursive calls"));
    assert_eq!(
        runtime.function_call_count_state(&seq_r_entry),
        VmCallCountState::Active { count: 1 }
    );
    assert_eq!(
        runtime.function_call_count_state(&seq_r_loop),
        VmCallCountState::Active { count: 3 }
    );
    assert_eq!(
        runtime
            .processes()
            .snapshot(actor)
            .expect("counted actor snapshot")
            .current_stacktrace,
        [VmProcessLocation {
            source: function("combo", 0),
            instruction_offset: 5,
        }]
    );
}
