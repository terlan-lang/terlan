use sha2::{Digest, Sha256};

use super::{
    VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome, VmSchedulerSlice,
};
use crate::runtime::vm::process::{VmProcessId, VmProcessSource, VmProcessState, VmProcessTable};

const BYTES_PER_REDUCTION: usize = 1024;
const REDUCTIONS_PER_SLICE: u64 = 64;

#[derive(Debug)]
struct YieldingHashReport {
    digest: [u8; 32],
    yielding_slices: usize,
    peer_slices_between_work: usize,
    worker_reductions: u64,
}

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("beam.yielding_c_fun", function, 0)
}

fn run_yielding_sha256(input: &[u8]) -> YieldingHashReport {
    let mut processes = VmProcessTable::default();
    let worker = processes.spawn_root(source("sha256"));
    let peer = processes.spawn_root(source("peer"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(REDUCTIONS_PER_SLICE, 4));
    scheduler
        .enqueue_runnable(&processes, worker)
        .expect("hash worker should enqueue");
    scheduler
        .enqueue_runnable(&processes, peer)
        .expect("peer should enqueue");

    let mut hasher = Sha256::new();
    let mut offset = 0;
    let mut digest = None;
    let mut yielding_slices = 0;
    let mut peer_slices_between_work = 0;

    while scheduler.queued_len() != 0 {
        let run = scheduler
            .run_next(&mut processes, |_process, slice| {
                run_hash_slice(
                    input,
                    worker,
                    slice,
                    &mut hasher,
                    &mut offset,
                    &mut digest,
                    &mut yielding_slices,
                    &mut peer_slices_between_work,
                )
            })
            .expect("yielding hash slice should run");
        assert_ne!(run.outcome, VmSchedulerOutcome::Idle);
    }

    assert_eq!(
        processes.get(worker).map(|process| &process.state),
        Some(&VmProcessState::Blocked)
    );
    assert_eq!(
        processes.get(peer).map(|process| &process.state),
        Some(&VmProcessState::Blocked)
    );

    YieldingHashReport {
        digest: digest.expect("final hash slice should publish a digest"),
        yielding_slices,
        peer_slices_between_work,
        worker_reductions: processes
            .get(worker)
            .expect("hash worker should remain observable")
            .reductions,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_hash_slice(
    input: &[u8],
    worker: VmProcessId,
    slice: VmSchedulerSlice,
    hasher: &mut Sha256,
    offset: &mut usize,
    digest: &mut Option<[u8; 32]>,
    yielding_slices: &mut usize,
    peer_slices_between_work: &mut usize,
) -> VmSchedulerDecision {
    if slice.pid != worker {
        if digest.is_some() {
            return VmSchedulerDecision::Block { reductions: 1 };
        }
        *peer_slices_between_work += 1;
        return VmSchedulerDecision::Yield { reductions: 1 };
    }

    let slice_bytes = (slice.reduction_budget as usize).saturating_mul(BYTES_PER_REDUCTION);
    let end = offset.saturating_add(slice_bytes).min(input.len());
    hasher.update(&input[*offset..end]);
    let processed = end - *offset;
    *offset = end;
    let reductions = processed.div_ceil(BYTES_PER_REDUCTION).max(1) as u64;

    if *offset == input.len() {
        *digest = Some(std::mem::take(hasher).finalize().into());
        VmSchedulerDecision::Block { reductions }
    } else {
        *yielding_slices += 1;
        VmSchedulerDecision::Yield { reductions }
    }
}

#[test]
fn yielding_sha256_retains_state_matches_fixture_vectors_and_schedules_peers() {
    let short = run_yielding_sha256(b"hej");
    assert_eq!(
        short.digest,
        [
            0x9c, 0x47, 0x8b, 0xf6, 0x3e, 0x95, 0x00, 0xcb, 0x5d, 0xb1, 0xe8, 0x5e, 0xce, 0x82,
            0xf1, 0x8c, 0x8e, 0xb9, 0xe5, 0x2e, 0x2f, 0x91, 0x35, 0xac, 0xd7, 0xf1, 0x09, 0x72,
            0xc8, 0xd5, 0x63, 0xba,
        ]
    );
    assert_eq!(short.yielding_slices, 0);
    assert_eq!(short.peer_slices_between_work, 0);
    assert_eq!(short.worker_reductions, 1);

    for shift in 1..15 {
        let size = 1024_usize << shift;
        let input = vec![b'h'; size];
        let expected: [u8; 32] = Sha256::digest(&input).into();
        let report = run_yielding_sha256(&input);
        let expected_slices = size.div_ceil(REDUCTIONS_PER_SLICE as usize * BYTES_PER_REDUCTION);
        let expected_yields = expected_slices.saturating_sub(1);

        assert_eq!(report.digest, expected, "fixture size {size}");
        assert_eq!(
            report.yielding_slices, expected_yields,
            "fixture size {size}"
        );
        assert_eq!(
            report.peer_slices_between_work, expected_yields,
            "peer must run once between every bounded hash slice for size {size}"
        );
        assert_eq!(
            report.worker_reductions,
            size.div_ceil(BYTES_PER_REDUCTION) as u64,
            "fixture size {size}"
        );
    }
}
