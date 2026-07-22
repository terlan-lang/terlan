//! Bounded deterministic frontend compilation for native applications.

use std::path::PathBuf;
use std::thread;

use crate::CliState;

use super::super::BuildOneError;
use super::compile::{compile_vm_module, CompiledVmModule};

/// Maximum number of frontend workers admitted by one application build.
const MAX_FRONTEND_WORKERS: usize = 8;

/// Failure produced by one bounded indexed task set.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum ParallelTaskError<E> {
    /// One indexed task returned its typed failure.
    Task(E),
    /// One worker panicked before returning its indexed outcomes.
    WorkerPanicked,
}

/// Compiles an application source closure with bounded frontend parallelism.
///
/// Inputs:
/// - `paths`: deterministically ordered implementation source paths.
/// - `state`: immutable compiler configuration shared by every worker.
///
/// Output:
/// - Checked modules in the exact input order.
/// - The lowest-indexed compile failure when any module fails.
///
/// Transformation:
/// - Bounds workers by host parallelism and a compiler-owned ceiling, compiles
///   independent modules against the prepared interface-summary cache, and
///   restores source order before the single application-link stage.
pub(super) fn compile_vm_modules(
    paths: &[PathBuf],
    state: &CliState,
) -> Result<Vec<CompiledVmModule>, BuildOneError> {
    let worker_limit = bounded_worker_limit();
    match run_indexed_bounded(paths, worker_limit, |path| {
        compile_vm_module(&path.to_string_lossy(), state)
    }) {
        Ok(modules) => Ok(modules),
        Err(ParallelTaskError::Task(error)) => Err(error),
        Err(ParallelTaskError::WorkerPanicked) => Err(BuildOneError::Message(
            "error[build.parallel_frontend_panic]: native application frontend worker panicked"
                .to_string(),
        )),
    }
}

/// Returns the shared host-bound worker ceiling for compiler-owned build work.
pub(super) fn bounded_worker_limit() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(MAX_FRONTEND_WORKERS)
}

/// Executes indexed work concurrently while preserving deterministic outcomes.
///
/// Inputs:
/// - `inputs`: immutable tasks whose slice position defines result order.
/// - `worker_limit`: positive preferred concurrency bound; zero is normalized
///   to one worker.
/// - `operation`: thread-safe task transformation.
///
/// Output:
/// - Successful values in input order.
/// - The lowest-indexed task failure, independent of worker completion order.
/// - `WorkerPanicked` when a worker exits without returning outcomes.
///
/// Transformation:
/// - Distributes indexes round-robin across scoped workers, joins every worker,
///   restores indexed slots, and only then resolves typed task results.
pub(super) fn run_indexed_bounded<I, O, E, F>(
    inputs: &[I],
    worker_limit: usize,
    operation: F,
) -> Result<Vec<O>, ParallelTaskError<E>>
where
    I: Sync,
    O: Send,
    E: Send,
    F: Fn(&I) -> Result<O, E> + Sync,
{
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let worker_count = worker_limit.max(1).min(inputs.len());
    thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for worker_index in 0..worker_count {
            let operation = &operation;
            workers.push(scope.spawn(move || {
                (worker_index..inputs.len())
                    .step_by(worker_count)
                    .map(|index| (index, operation(&inputs[index])))
                    .collect::<Vec<_>>()
            }));
        }

        let mut indexed = std::iter::repeat_with(|| None)
            .take(inputs.len())
            .collect::<Vec<_>>();
        let mut worker_panicked = false;
        for worker in workers {
            match worker.join() {
                Ok(outcomes) => {
                    for (index, outcome) in outcomes {
                        indexed[index] = Some(outcome);
                    }
                }
                Err(_) => worker_panicked = true,
            }
        }
        if worker_panicked {
            return Err(ParallelTaskError::WorkerPanicked);
        }

        indexed
            .into_iter()
            .map(|outcome| {
                outcome
                    .expect("every indexed task is assigned to exactly one worker")
                    .map_err(ParallelTaskError::Task)
            })
            .collect()
    })
}
