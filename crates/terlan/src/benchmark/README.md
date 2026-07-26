# Benchmark Internals

This directory owns Terlan benchmark probes that compare compiler/runtime lanes
without making benchmark results part of the public compiler API. The
implementation is centered on small typed benchmark reports that can be run from
the `terlan-benchmark` binary. Its most important boundary is that benchmarks
may observe runtime behavior, but they must not become required runtime
machinery.

## Responsibilities

- Provide reproducible benchmark scenarios for runtime and compiler migration
  work.
- Keep benchmark inputs small, explicit, and correct before measuring timing.
- Report lane capability status when a benchmark target is available or not
  implemented yet.
- Avoid hiding runtime gaps behind synthetic success results.

## Public Surface

- `main.rs`: command-line entry point for the benchmark binary.
- `binary_protocol.rs`: correctness-checked binary encode/decode, VM framing,
  and VM HTTP lifecycle workloads with cold/warm JSON report generation.
- `runtime_workloads.rs`: fixed direct-AOT allocation, messaging, scheduling,
  collection-pause, actor-churn, and mixed-tail runtime workloads.
- `managed_heap.rs`: safe benchmark embedding of the canonical actor heap,
  layout, root, and mailbox modules; it defines no benchmark-only VM types.
- `scripts/benchmarks/protocol/`: fixed workload/seed definitions, comparison
  driver, JSON/TSV publication, and stable winner/delta validation.
- `http_runtime_lane.rs`: HTTP runtime lane capability and report helpers.
- `http_aot_performance.rs`: executable native-AOT HTTP lane recorder.
- `http_framework_baseline.rs`: framework-neutral maintained-client recorder
  used for both Axum and plain Hyper controls.
- `http_paired_benchmark.rs`: rotating AOT/Axum/plain-Hyper orchestration,
  environment admission, paired bootstrap statistics, and isolation evidence.
- `aot_compilation.rs`: equivalent Terlan/Go small-command and multi-package
  compilation recorder covering cold development, edits, no-op reuse, release,
  relinking, compiler-service startup, and REPL generations.
- `native_modules.rs`: native module benchmark metadata and fixtures.

## Core Model

Benchmark modules build typed scenario descriptions and convert them into
human-readable reports. A report can include measured timing when a lane is
implemented, or a stable unavailable status when the runtime path is not ready
to execute the scenario. The VM HTTP lane has a correctness gate in normal
checks and a live loopback timing run behind `TERLAN_VM_HTTP_LIVE=1`.

The main flow is:

1. Select a benchmark scenario from the benchmark binary.
2. Build the lane-specific runtime plan or capability status.
3. Render a report that can be compared across future runtime changes.

Important invariants:

- Benchmarks must distinguish unavailable runtime capability from failure.
- Benchmark scenarios must keep correctness assertions near the measured path.
- Benchmark code must not introduce production dependencies into compiler or VM
  execution paths.
- Runtime workload timing must use the checked
  `benchmarks/baselines/vm-aot-runtime-workloads.json` manifest. Workload names,
  order, sample count, operation count, and scope are part of that reference.
- Runtime workload reports record throughput and p50/p95/p99/max latency. The
  release gate validates execution and report shape without imposing a
  machine-independent duration threshold.
- HTTP AOT performance reports use the same source package and workload for
  both lanes. They record a hashed OS, architecture, CPU, logical-core, and
  Rust toolchain fingerprint; request latency and throughput; process resident
  memory; concurrent pressure; sustained request longevity; and live source
  generation replacement. Comparison rejects missing evidence, different
  workloads, mixed hardware fingerprints, and any regression outside the
  versioned quantitative policy in
  `benchmarks/baselines/http-aot-performance-limits.json`. The comparison
  embeds the exact policy and its digest so changing a budget is reviewable.

## Integration Points

- `runtime::vm`: supplies VM execution capability status and future execution
  hooks.
- `runtime::native`: supplies Rust-owned native adapters used by benchmark
  scenarios.
- `commands::serve`: defines HTTP behavior that benchmark probes must not
  duplicate.

## Edge Cases

- A not-yet-implemented Terlan VM lane is reported as typed unavailable, not as
  a zero-duration success.
- Live loopback HTTP timing requires local TCP bind permission and stays
  opt-in for restricted runners.
- HTTP benchmark probes must not hand-roll protocol parsing or server behavior.
- Binary protocol cold timings include compiler-process startup and VM test
  execution; warm samples repeat the same fixed workload after the first cold
  population and retain an explicit process-plus-workload measurement scope.
- VM framing and HTTP lifecycle timing use standalone VM commands so compiler
  startup is outside their internal measurement. HTTP concurrency uses
  independently admitted VM logical streams and does not require host socket
  permission.
- Every comparable row prints and stores its winner and signed delta against
  the checked baseline. Structurally different legacy lanes remain explicitly
  unsupported rather than receiving synthetic numbers.
- Native-boundary measurements must keep capability names and failure modes
  stable so future regressions are visible.

## Types And Interfaces

`HttpRuntimeLane`
: Identifies the runtime lane being described by an HTTP benchmark report.

`HttpRuntimeReport`
: Captures whether an HTTP benchmark lane is executable, unavailable, or failed
  with a stable diagnostic.

## Testing Notes

- `http_runtime_lane.rs` contains focused tests for typed VM HTTP lane
  reporting.
- `http-aot-performance-self-test` validates timing, comparison, fingerprint,
  incomplete-evidence rejection, every performance-budget dimension, and the
  hard policy ceilings without requiring a loopback socket.
- `make tvm-aot-http-performance-check` records the native lane and rejects
  latency, throughput, peak resident-memory, or generation-reload ratios that
  exceed the committed checked-CoreIR comparison policy.
- `make tvm-http-paired-performance-check` rotates all six
  native-AOT/Axum/plain-Hyper orders, rejects samples contaminated on reserved
  CPUs, validates every decisive route and payload with maintained `curl` and
  `wrk`, and records deterministic 10,000-resample bootstrap intervals. The
  internal client rows remain diagnostic and cannot decide the comparison when
  maintained workload evidence is present.
- `make tvm-http-decisive-performance-check` requires at least ten paired
  10-second samples, explicit disjoint server/client CPU sets, the performance
  governor, a single server NUMA node, and no IRQ affinity overlap. It fails
  before measurement when those controls are not satisfied; the shorter target
  emits explicitly advisory evidence.
- The maintained matrix covers sequential and pressure traffic, bounded
  oversubscription, keep-alive and connection teardown, empty/4KiB/64KiB/1MiB
  bodies, header pressure, JSON parsing, request metadata, static responses,
  errors, and cancelled slow uploads. Optional `wrk2` offered-load sweeps avoid
  coordinated omission. `TERLAN_BENCH_HTTP_TLS_URL` adds maintained TLS and
  HTTP/2 negotiation evidence without pretending plaintext HTTP/1.1 proves it.
- Reports attribute idle, warmed, peak, retained RSS/PSS, thread count, process
  efficiency, soak growth, exact binary and Cargo.lock hashes, target CPU, LTO,
  codegen-unit, panic, allocator, and socket policies. Paired comparison rejects
  unequal workload, hardware, build, affinity, or socket-policy evidence.
- Long-form paired runs are controlled by `TERLAN_BENCH_HTTP_PAIRS`,
  `TERLAN_BENCH_HTTP_DURATION_MS`, and `TERLAN_BENCH_HTTP_SOAK_SECONDS`.
  Optional hardware counters use `TERLAN_BENCH_HTTP_PERF_SECONDS`; unavailable
  kernel permissions are recorded explicitly rather than fabricating zeros.
- Add focused tests when new benchmark report states are introduced.
- Run `terlan-benchmark aot-compilation-self-test` for production-compiled
  report, percentile, malformed-evidence, serialization, and fixture checks.
- Run `terlan-benchmark aot-compilation` with release-built `terlc` and
  `terlan-vm` siblings to record the complete same-machine compilation report.
- Run `terlan-benchmark aot-compilation-validate` to enforce the committed
  cold and incremental Terlan-to-Go ratio ceilings and warm p95 limit from
  `benchmarks/baselines/aot-compilation-limits.json`.
- Keep large performance sweeps outside normal unit tests; release gates should
  validate shape and correctness, not depend on machine-specific timing.
