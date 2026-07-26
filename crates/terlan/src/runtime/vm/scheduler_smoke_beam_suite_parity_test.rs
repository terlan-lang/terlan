//! Portable replacement coverage for OTP's retired `smoke_test_SUITE`.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::*;
use crate::runtime::vm::actor::VmActorRuntime;
use crate::runtime::vm::process::{VmExitReason, VmProcessSource};
use crate::runtime::vm::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};

/// Replaces the BEAM boot-flag matrix with every representative Terlan
/// scheduler width, deterministic placement, and actual owner-thread work.
#[test]
fn smoke_suite_scheduler_profiles_boot_and_execute_owned_work() {
    for width in [1, 2, 4, VM_MAX_SCHEDULERS] {
        let topology = VmSchedulerTopology::new(width).expect("smoke topology");
        assert_eq!(topology.width(), width);
        for scheduler in topology.schedulers() {
            let actor_id = NonZeroU64::new(scheduler.index() as u64 + 1).expect("actor id");
            let route = topology.route(actor_id);
            assert_eq!(route.home_scheduler(), scheduler);
            assert_eq!(route.scheduler(), scheduler);

            let mut runtime = VmActorRuntime::with_scheduler_owner(scheduler.owner_word())
                .expect("scheduler-owned runtime");
            let pid = runtime.spawn_root(VmProcessSource::new(
                "parity.Smoke",
                format!("scheduler_{}", scheduler.index()),
                0,
            ));
            let run = runtime
                .run_next(|process, _| {
                    assert_eq!(process.pid, pid);
                    VmSchedulerDecision::Exit {
                        reductions: 1,
                        reason: VmExitReason::Normal,
                    }
                })
                .expect("smoke actor execution");
            assert_eq!(run.pid, Some(pid));
            assert!(matches!(run.outcome, VmSchedulerOutcome::Exited(_)));
        }
    }
}

/// Preserves the optimized-runtime atomics requirement through the primitives
/// used by Terlan mailbox publication, ownership, and scheduler accounting.
#[test]
fn smoke_suite_requires_native_32_and_64_bit_atomic_progress() {
    assert!(
        cfg!(target_has_atomic = "32"),
        "Terlan's multicore VM requires native 32-bit atomics"
    );
    assert!(
        cfg!(target_has_atomic = "64"),
        "Terlan's multicore VM requires native 64-bit atomics"
    );

    let publications = Arc::new(AtomicU64::new(0));
    let completed = Arc::new(AtomicU32::new(0));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let publications = Arc::clone(&publications);
            let completed = Arc::clone(&completed);
            scope.spawn(move || {
                for _ in 0..10_000 {
                    publications.fetch_add(1, Ordering::AcqRel);
                }
                completed.fetch_add(1, Ordering::Release);
            });
        }
    });

    assert_eq!(completed.load(Ordering::Acquire), 8);
    assert_eq!(publications.load(Ordering::Acquire), 80_000);
}
