//! Tests for bounded deterministic frontend compilation.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Barrier;

use super::parallel_compile::{run_indexed_bounded, ParallelTaskError};

/// Proves bounded workers execute concurrently and restore input order.
#[test]
fn indexed_parallel_tasks_are_bounded_and_deterministically_ordered() {
    let inputs = (0usize..8).collect::<Vec<_>>();
    let barrier = Barrier::new(4);
    let active = AtomicUsize::new(0);
    let maximum = AtomicUsize::new(0);

    let outputs = run_indexed_bounded(&inputs, 4, |value| {
        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
        maximum.fetch_max(current, Ordering::SeqCst);
        if *value < 4 {
            barrier.wait();
        }
        active.fetch_sub(1, Ordering::SeqCst);
        Ok::<_, ()>(value * 10)
    })
    .expect("execute bounded tasks");

    assert_eq!(outputs, vec![0, 10, 20, 30, 40, 50, 60, 70]);
    assert_eq!(maximum.load(Ordering::SeqCst), 4);
}

/// Proves zero worker limits normalize safely and empty sets spawn no work.
#[test]
fn indexed_parallel_tasks_normalize_zero_workers_and_empty_inputs() {
    let inputs = vec![3usize, 1, 2];
    let outputs = run_indexed_bounded(&inputs, 0, |value| Ok::<_, ()>(value + 1))
        .expect("execute normalized single worker");
    assert_eq!(outputs, vec![4, 2, 3]);

    let empty = run_indexed_bounded::<usize, usize, (), _>(&[], 8, |value| Ok(*value))
        .expect("execute empty task set");
    assert!(empty.is_empty());
}

/// Proves task failure selection follows source order, not worker join order.
#[test]
fn indexed_parallel_tasks_return_the_lowest_indexed_failure() {
    let inputs = (0usize..8).collect::<Vec<_>>();

    let error = run_indexed_bounded(&inputs, 4, |value| match value {
        2 => Err("source-two"),
        5 => Err("source-five"),
        _ => Ok(*value),
    })
    .expect_err("task set must fail");

    assert_eq!(error, ParallelTaskError::Task("source-two"));
}

/// Proves simultaneous worker panics become one typed orchestration failure.
#[test]
fn indexed_parallel_tasks_report_worker_panics() {
    let inputs = vec![0usize, 1, 2];

    let error = run_indexed_bounded(&inputs, 3, |value| {
        assert_eq!(*value, 0, "intentional worker panic");
        Ok::<_, ()>(*value)
    })
    .expect_err("worker panic must fail the task set");

    assert_eq!(error, ParallelTaskError::WorkerPanicked);
}
