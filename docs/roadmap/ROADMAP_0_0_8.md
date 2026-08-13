# Terlan 0.0.8 Roadmap

Most of this roadmap begins after the 0.0.7 candidate is sealed. The validation
throughput foundation in V8-1 was pulled forward into 0.0.7 Slice 70 because a
monolithic same-run closeout proved operationally unreasonable; 0.0.8 retains
the broader tiering, cleanup, measurement, and ratcheting follow-through. Work
is selected in document order. Accelerator-specific work remains owned by
`ROADMAP_0_0_8_CUDA.md`; this file owns cross-cutting compiler, VM, tooling,
and release work.

## Validation Throughput And Evidence Reuse

- [ ] V8-1: make exhaustive validation fast without weakening release evidence.
  - Split Rust validation into explicitly inventoried fast unit, integration,
    AOT/native-link, concurrency/timeout, performance, and controlled-host
    tiers. Every test belongs to exactly one tier, and the release aggregate
    still executes every required tier.
  - Run EOF-dependent CLI, REPL, and debugger tests with closed plain pipes.
    No automated gate may inherit a live terminal accidentally. Add an
    adversarial test that fails quickly when a child waits for undeclared
    interactive input.
  - Build each required compiler/profile/feature artifact once per validation
    cycle. Later gates consume the sealed artifact; invoking an equivalent
    Cargo, Terlan AOT, native-link, or self-host build twice in one cycle is an
    error.
  - Add a generation-safe, content-addressed within-cycle cache for identical
    Terlan AOT and native-link inputs. Its key includes compiler and runtime ABI
    identities, normalized typed input, target, profile, features, dependency
    lock, and relevant environment policy. Stale, incomplete, cross-target, or
    post-seal-mutated entries fail closed.
  - Register every temporary checkout, target directory, native-link workspace,
    package cache, and test artifact with the validation-cycle owner. Remove it
    on success, assertion failure, panic, timeout, cancellation, and signal
    termination. Before each tier or measurement lane, reject and attribute
    orphaned partial builds from the preceding step instead of accumulating or
    silently reusing them.
  - Keep reusable sealed caches separate from disposable test workspaces. Apply
    explicit byte/entry/age budgets and generation-safe garbage collection;
    cleanup may never delete source, the active sealed candidate, or evidence
    required by a later gate.
  - Keep concurrency and performance evidence isolated from parallel work that
    could distort results. Parallelize independent deterministic tiers only
    where their contracts permit it; preserve stable reporting order.
  - Emit one machine-readable validation timing/duplication report with tier,
    test count, compile count, cache hits/misses, wall time, CPU time, peak
    memory, artifact bytes, and the slowest tests/builds. Record the 0.0.7
    closeout as the initial baseline.
  - Add ratcheted budgets for duplicate builds, per-tier wall time, total
    preflight time, and cache correctness. A speedup may not remove tests,
    loosen assertions, reuse evidence across incompatible inputs, or conceal
    skips and timeouts.
  - Acceptance: a clean exhaustive preflight and a no-op warm preflight produce
    equivalent release decisions; the warm run performs no duplicate
    equivalent build, EOF-dependent tests terminate deterministically, all
    required tests remain inventoried, no disposable workspace survives its
    owning lane, and the report identifies every remaining dominant cost.
