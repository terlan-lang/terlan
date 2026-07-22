//! Deterministic schedule models for the thread-neutral AOT runtime contract.

use std::fmt::Debug;

/// Completed states and schedule count produced by one exhaustive exploration.
#[derive(Debug)]
struct Exploration<State> {
    /// Terminal state reached by every valid interleaving.
    terminals: Vec<State>,
    /// Number of valid schedules explored.
    schedule_count: usize,
}

/// Explores every enabled interleaving while preserving each participant's order.
fn explore_interleavings<State, Operation, Apply>(
    programs: &[&[Operation]],
    initial: State,
    apply: Apply,
) -> Exploration<State>
where
    State: Clone + Debug,
    Operation: Copy + Debug,
    Apply: Fn(&State, Operation) -> Option<State>,
{
    let mut positions = vec![0; programs.len()];
    let mut terminals = Vec::new();
    let mut schedule = Vec::new();
    explore_from(
        programs,
        &mut positions,
        initial,
        &apply,
        &mut schedule,
        &mut terminals,
    );
    Exploration {
        schedule_count: terminals.len(),
        terminals,
    }
}

/// Recursively advances each currently enabled participant by one operation.
fn explore_from<State, Operation, Apply>(
    programs: &[&[Operation]],
    positions: &mut [usize],
    state: State,
    apply: &Apply,
    schedule: &mut Vec<(usize, Operation)>,
    terminals: &mut Vec<State>,
) where
    State: Clone + Debug,
    Operation: Copy + Debug,
    Apply: Fn(&State, Operation) -> Option<State>,
{
    if positions
        .iter()
        .zip(programs)
        .all(|(position, program)| *position == program.len())
    {
        terminals.push(state);
        return;
    }

    let mut advanced = false;
    for participant in 0..programs.len() {
        let position = positions[participant];
        let Some(operation) = programs[participant].get(position).copied() else {
            continue;
        };
        let Some(next) = apply(&state, operation) else {
            continue;
        };
        advanced = true;
        positions[participant] += 1;
        schedule.push((participant, operation));
        explore_from(programs, positions, next, apply, schedule, terminals);
        schedule.pop();
        positions[participant] -= 1;
    }

    assert!(
        advanced,
        "deterministic schedule model deadlocked at {state:?} after {schedule:?}"
    );
}

/// Identity of one contender for a parked continuation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resumer {
    /// First scheduler contender.
    First,
    /// Second scheduler contender.
    Second,
}

/// One linearized operation against a parked continuation.
#[derive(Clone, Copy, Debug)]
enum ResumeOperation {
    /// Attempts to consume the pending continuation authority.
    Claim(Resumer),
    /// Attempts to resume after the claim result is known.
    Resume(Resumer),
}

/// Claim state retained for one contender.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClaimState {
    /// The contender has not reached the claim point.
    Unattempted,
    /// The contender consumed the linear claim.
    Won,
    /// Another contender consumed the linear claim first.
    Rejected,
}

/// Minimal linear-authority model for exactly-once continuation resume.
#[derive(Clone, Debug)]
struct ResumeModel {
    /// Whether the continuation remains available for an atomic claim.
    pending: bool,
    /// Claim result for the first contender.
    first: ClaimState,
    /// Claim result for the second contender.
    second: ClaimState,
    /// Contender that performed the only accepted resume.
    resumed_by: Option<Resumer>,
}

impl ResumeModel {
    /// Applies one atomic claim or one claim-dependent resume operation.
    fn apply(&self, operation: ResumeOperation) -> Option<Self> {
        let mut next = self.clone();
        match operation {
            ResumeOperation::Claim(resumer) => {
                let claim = if next.pending {
                    next.pending = false;
                    ClaimState::Won
                } else {
                    ClaimState::Rejected
                };
                *next.claim_mut(resumer) = claim;
            }
            ResumeOperation::Resume(resumer) => match next.claim(resumer) {
                ClaimState::Won if next.resumed_by.is_none() => next.resumed_by = Some(resumer),
                ClaimState::Rejected => {}
                ClaimState::Unattempted | ClaimState::Won => return None,
            },
        }
        Some(next)
    }

    /// Returns one contender's current claim state.
    fn claim(&self, resumer: Resumer) -> ClaimState {
        match resumer {
            Resumer::First => self.first,
            Resumer::Second => self.second,
        }
    }

    /// Mutably borrows one contender's claim state.
    fn claim_mut(&mut self, resumer: Resumer) -> &mut ClaimState {
        match resumer {
            Resumer::First => &mut self.first,
            Resumer::Second => &mut self.second,
        }
    }
}

/// Proves every contender ordering admits exactly one continuation resume.
#[test]
fn deterministic_model_rejects_double_resume() {
    let first = [
        ResumeOperation::Claim(Resumer::First),
        ResumeOperation::Resume(Resumer::First),
    ];
    let second = [
        ResumeOperation::Claim(Resumer::Second),
        ResumeOperation::Resume(Resumer::Second),
    ];
    let exploration = explore_interleavings(
        &[&first, &second],
        ResumeModel {
            pending: true,
            first: ClaimState::Unattempted,
            second: ClaimState::Unattempted,
            resumed_by: None,
        },
        ResumeModel::apply,
    );

    assert_eq!(exploration.schedule_count, 6);
    for terminal in exploration.terminals {
        assert!(!terminal.pending);
        assert_eq!(
            [terminal.first, terminal.second]
                .into_iter()
                .filter(|claim| *claim == ClaimState::Won)
                .count(),
            1
        );
        assert!(terminal.resumed_by.is_some());
    }
}

/// Operations participating in receive parking and mailbox publication.
#[derive(Clone, Copy, Debug)]
enum WakeOperation {
    /// Publishes complete message state before scheduler notification.
    Publish,
    /// Notifies a parked receiver after publication.
    Wake,
    /// Performs the receiver's first queue observation.
    Inspect,
    /// Parks only when no message has been observed.
    Park,
    /// Closes the inspect-to-park race with a second observation.
    Recheck,
}

/// Publish-before-wake model that prevents a receive-side lost wakeup.
#[derive(Clone, Debug, Default)]
struct WakeModel {
    /// Whether the complete message is visible to the receiver.
    published: bool,
    /// Whether the receiver has consumed visibility of the message.
    observed: bool,
    /// Whether the receiver is currently parked.
    parked: bool,
    /// Whether the receiver can run after observation or notification.
    runnable: bool,
}

impl WakeModel {
    /// Applies one sender or receiver operation at its linearization point.
    fn apply(&self, operation: WakeOperation) -> Option<Self> {
        let mut next = self.clone();
        match operation {
            WakeOperation::Publish => next.published = true,
            WakeOperation::Wake => {
                if next.parked {
                    next.parked = false;
                    next.runnable = true;
                }
            }
            WakeOperation::Inspect => {
                if next.published {
                    next.observed = true;
                    next.runnable = true;
                }
            }
            WakeOperation::Park => {
                if !next.observed {
                    next.parked = true;
                    next.runnable = false;
                }
            }
            WakeOperation::Recheck => {
                if next.published {
                    next.observed = true;
                    next.parked = false;
                    next.runnable = true;
                }
            }
        }
        Some(next)
    }
}

/// Proves publication, wake, park, and recheck cannot strand a visible message.
#[test]
fn deterministic_model_prevents_lost_wakeup() {
    let sender = [WakeOperation::Publish, WakeOperation::Wake];
    let receiver = [
        WakeOperation::Inspect,
        WakeOperation::Park,
        WakeOperation::Recheck,
    ];
    let exploration = explore_interleavings(
        &[&sender, &receiver],
        WakeModel::default(),
        WakeModel::apply,
    );

    assert_eq!(exploration.schedule_count, 10);
    for terminal in exploration.terminals {
        assert!(terminal.published);
        assert!(terminal.runnable);
        assert!(!terminal.parked);
    }
}

/// Exclusive owner of one mutable actor context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextOwner {
    /// Scheduler thread releasing the context.
    Old,
    /// Scheduler thread acquiring the context.
    New,
}

/// Operations in a release/acquire actor-context transfer.
#[derive(Clone, Copy, Debug)]
enum HandoffOperation {
    /// Begins the old owner's final mutation.
    OldBegin,
    /// Ends the old owner's final mutation.
    OldEnd,
    /// Releases ownership after all old-owner mutations.
    Release,
    /// Acquires ownership after release publication.
    Acquire,
    /// Begins the new owner's first mutation.
    NewBegin,
    /// Ends the new owner's first mutation.
    NewEnd,
}

/// Linear ownership model for one mutable actor execution context.
#[derive(Clone, Debug)]
struct HandoffModel {
    /// Current context owner, or no owner during transfer.
    owner: Option<ContextOwner>,
    /// Owner currently mutating the context.
    active: Option<ContextOwner>,
    /// Whether the old owner completed release publication.
    released: bool,
    /// Whether the new owner completed its first mutation.
    new_completed: bool,
}

impl HandoffModel {
    /// Applies one ownership operation only when its preconditions hold.
    fn apply(&self, operation: HandoffOperation) -> Option<Self> {
        let mut next = self.clone();
        match operation {
            HandoffOperation::OldBegin
                if next.owner == Some(ContextOwner::Old) && next.active.is_none() =>
            {
                next.active = Some(ContextOwner::Old);
            }
            HandoffOperation::OldEnd if next.active == Some(ContextOwner::Old) => {
                next.active = None;
            }
            HandoffOperation::Release
                if next.owner == Some(ContextOwner::Old) && next.active.is_none() =>
            {
                next.owner = None;
                next.released = true;
            }
            HandoffOperation::Acquire
                if next.released && next.owner.is_none() && next.active.is_none() =>
            {
                next.owner = Some(ContextOwner::New);
            }
            HandoffOperation::NewBegin
                if next.owner == Some(ContextOwner::New) && next.active.is_none() =>
            {
                next.active = Some(ContextOwner::New);
            }
            HandoffOperation::NewEnd if next.active == Some(ContextOwner::New) => {
                next.active = None;
                next.new_completed = true;
            }
            _ => return None,
        }
        Some(next)
    }
}

/// Proves a mutable context cannot be observed by its new owner before release.
#[test]
fn deterministic_model_linearizes_actor_context_handoff() {
    let old = [
        HandoffOperation::OldBegin,
        HandoffOperation::OldEnd,
        HandoffOperation::Release,
    ];
    let new = [
        HandoffOperation::Acquire,
        HandoffOperation::NewBegin,
        HandoffOperation::NewEnd,
    ];
    let exploration = explore_interleavings(
        &[&old, &new],
        HandoffModel {
            owner: Some(ContextOwner::Old),
            active: None,
            released: false,
            new_completed: false,
        },
        HandoffModel::apply,
    );

    assert_eq!(exploration.schedule_count, 1);
    let terminal = &exploration.terminals[0];
    assert_eq!(terminal.owner, Some(ContextOwner::New));
    assert!(terminal.active.is_none());
    assert!(terminal.released);
    assert!(terminal.new_completed);
}

/// Lifecycle of an actor racing with continuation completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Lifecycle {
    /// Actor may still consume a claimed continuation.
    Alive,
    /// Actor exit has linearized and later completion must be dropped.
    Exited,
}

/// Lifecycle of the parked continuation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContinuationState {
    /// Continuation remains available for a completion claim.
    Pending,
    /// Completion owns the continuation authority.
    Claimed,
    /// Exit cleanup released the continuation authority.
    Released,
}

/// Result observed by the completion participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompletionState {
    /// Completion has not attempted a claim.
    Unstarted,
    /// Completion owns the pending continuation.
    Claimed,
    /// Completion resumed while the actor was alive.
    Resumed,
    /// Completion lost to actor exit and was discarded.
    Dropped,
}

/// Operations in the unified exit-versus-completion pipeline.
#[derive(Clone, Copy, Debug)]
enum ExitOperation {
    /// Linearizes actor exit.
    MarkExit,
    /// Releases remaining continuation ownership.
    ReleaseContinuation,
    /// Attempts to claim the parked continuation.
    ClaimCompletion,
    /// Attempts to resume using a successful claim.
    ResumeCompletion,
}

/// Model retaining enough ordering evidence to reject post-exit resume.
#[derive(Clone, Debug)]
struct ExitModel {
    /// Current actor lifecycle.
    lifecycle: Lifecycle,
    /// Current continuation lifecycle.
    continuation: ContinuationState,
    /// Completion participant state.
    completion: CompletionState,
    /// Monotonic model step.
    clock: usize,
    /// Step at which exit linearized.
    exit_at: Option<usize>,
    /// Step at which resume linearized.
    resume_at: Option<usize>,
}

impl ExitModel {
    /// Applies one exit or completion operation with fail-closed late completion.
    fn apply(&self, operation: ExitOperation) -> Option<Self> {
        let mut next = self.clone();
        next.clock += 1;
        match operation {
            ExitOperation::MarkExit if next.lifecycle == Lifecycle::Alive => {
                next.lifecycle = Lifecycle::Exited;
                next.exit_at = Some(next.clock);
            }
            ExitOperation::ReleaseContinuation if next.lifecycle == Lifecycle::Exited => {
                next.continuation = ContinuationState::Released;
                if next.completion == CompletionState::Claimed {
                    next.completion = CompletionState::Dropped;
                }
            }
            ExitOperation::ClaimCompletion if next.completion == CompletionState::Unstarted => {
                if next.lifecycle == Lifecycle::Alive
                    && next.continuation == ContinuationState::Pending
                {
                    next.continuation = ContinuationState::Claimed;
                    next.completion = CompletionState::Claimed;
                } else {
                    next.completion = CompletionState::Dropped;
                }
            }
            ExitOperation::ResumeCompletion => match next.completion {
                CompletionState::Claimed
                    if next.lifecycle == Lifecycle::Alive
                        && next.continuation == ContinuationState::Claimed =>
                {
                    next.completion = CompletionState::Resumed;
                    next.resume_at = Some(next.clock);
                }
                CompletionState::Claimed | CompletionState::Dropped => {
                    next.completion = CompletionState::Dropped;
                }
                CompletionState::Unstarted | CompletionState::Resumed => return None,
            },
            _ => return None,
        }
        Some(next)
    }
}

/// Proves completion either precedes exit or is dropped and cleaned up.
#[test]
fn deterministic_model_rejects_resume_after_actor_exit() {
    let exiting = [ExitOperation::MarkExit, ExitOperation::ReleaseContinuation];
    let completion = [
        ExitOperation::ClaimCompletion,
        ExitOperation::ResumeCompletion,
    ];
    let exploration = explore_interleavings(
        &[&exiting, &completion],
        ExitModel {
            lifecycle: Lifecycle::Alive,
            continuation: ContinuationState::Pending,
            completion: CompletionState::Unstarted,
            clock: 0,
            exit_at: None,
            resume_at: None,
        },
        ExitModel::apply,
    );

    assert_eq!(exploration.schedule_count, 6);
    for terminal in exploration.terminals {
        assert_eq!(terminal.lifecycle, Lifecycle::Exited);
        assert_eq!(terminal.continuation, ContinuationState::Released);
        if let Some(resume_at) = terminal.resume_at {
            assert!(resume_at < terminal.exit_at.expect("exit linearized"));
            assert_eq!(terminal.completion, CompletionState::Resumed);
        } else {
            assert_eq!(terminal.completion, CompletionState::Dropped);
        }
    }
}

/// One independent shard operation used to model lock contention.
#[derive(Clone, Copy, Debug)]
enum ShardOperation {
    /// Acquires shard A's local ownership token.
    AcquireA,
    /// Executes one unit of shard A work.
    WorkA,
    /// Releases shard A's local ownership token.
    ReleaseA,
    /// Acquires shard B's local ownership token.
    AcquireB,
    /// Executes one unit of shard B work.
    WorkB,
    /// Releases shard B's local ownership token.
    ReleaseB,
}

/// Independent-lock model proving one shard cannot serialize another.
#[derive(Clone, Debug, Default)]
struct ShardModel {
    /// Whether shard A's local token is held.
    held_a: bool,
    /// Whether shard B's local token is held.
    held_b: bool,
    /// Whether shard A completed work.
    completed_a: bool,
    /// Whether shard B completed work.
    completed_b: bool,
    /// Whether both shards held their independent tokens concurrently.
    overlapped: bool,
}

impl ShardModel {
    /// Applies one shard-local ownership operation.
    fn apply(&self, operation: ShardOperation) -> Option<Self> {
        let mut next = self.clone();
        match operation {
            ShardOperation::AcquireA if !next.held_a => next.held_a = true,
            ShardOperation::WorkA if next.held_a => next.completed_a = true,
            ShardOperation::ReleaseA if next.held_a => next.held_a = false,
            ShardOperation::AcquireB if !next.held_b => next.held_b = true,
            ShardOperation::WorkB if next.held_b => next.completed_b = true,
            ShardOperation::ReleaseB if next.held_b => next.held_b = false,
            _ => return None,
        }
        next.overlapped |= next.held_a && next.held_b;
        Some(next)
    }
}

/// Proves independent shard ownership permits progress and overlapping execution.
#[test]
fn deterministic_model_has_no_process_global_lock() {
    let shard_a = [
        ShardOperation::AcquireA,
        ShardOperation::WorkA,
        ShardOperation::ReleaseA,
    ];
    let shard_b = [
        ShardOperation::AcquireB,
        ShardOperation::WorkB,
        ShardOperation::ReleaseB,
    ];
    let exploration = explore_interleavings(
        &[&shard_a, &shard_b],
        ShardModel::default(),
        ShardModel::apply,
    );

    assert_eq!(exploration.schedule_count, 20);
    assert!(exploration.terminals.iter().any(|state| state.overlapped));
    for terminal in exploration.terminals {
        assert!(terminal.completed_a && terminal.completed_b);
        assert!(!terminal.held_a && !terminal.held_b);
    }
}
