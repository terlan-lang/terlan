# Terlan 0.0.7 Roadmap

This is the active release-blocking execution plan. Completed work and detailed
progress narration are intentionally absent. The prior full document is preserved
in
[`archive/ROADMAP_0_0_7_ACTIVE_PRE_COMPACTION_2026_07_31.md`](archive/ROADMAP_0_0_7_ACTIVE_PRE_COMPACTION_2026_07_31.md),
and older implementation history remains in
[`archive/ROADMAP_0_0_7_IMPLEMENTED.md`](archive/ROADMAP_0_0_7_IMPLEMENTED.md).

The code-quality roadmap and direct-AOT pivot are complete. The remaining 0.0.7
work closes runtime semantics, production behavior, the developer loop, formal
evidence, and local release validation. It does not require a commit, push, tag,
upload, registry publication, hosted service, or external account.

## Release Boundary

0.0.7 ships Terlan as a direct-AOT language and VM runtime:

- Cranelift emits target-native TVM executable images from checked Terlan IR.
- The VM owns actor scheduling, mailboxes, timers, cancellation, supervision,
  resource lifecycle, observability, and AOT image admission.
- Same-shard operations use typed runtime ABI calls; pointer-free transport is
  reserved for real isolation boundaries.
- Rust, C, C++, and CUDA packages expose safe typed Terlan capability APIs:
  raw pointers, driver handles, native ownership, and unchecked device memory
  never cross the package boundary. Their inherently unsafe FFI/driver
  implementations execute through asynchronous capability workers and cannot
  block or corrupt a shard owner.
- Tokio, BEAM, OTP, serialized VMIR, CoreIR evaluation, and interpreter paths do
  not define runtime semantics.
- Maintained protocol/database crates own wire formats. Terlan owns scheduling,
  lifecycle, backpressure, capabilities, typed failures, and integration.
- JavaScript lowering remains a separate compiler target. VM actor scheduling
  semantics are not imposed on JavaScript output.

The normative runtime artifact contract is
[`TVM_EXECUTABLE_IMAGE_SPEC.md`](../runtime/TVM_EXECUTABLE_IMAGE_SPEC.md).
The current native data and call contract is ABI 1 in
[`TVM_NATIVE_DATA_ABI_SPEC.md`](../../terlan/docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md).

## Native ABI 1 Current Pre-Freeze Contract

0.0.7 ships ABI 1 as the sole current implementation but does not freeze
cross-release binary compatibility. Correctness and containment are release
requirements now; optimization and compatibility stability belong to 0.0.9.

| Requirement | Current implementation owner | 0.0.7 evidence |
| --- | --- | --- |
| Canonical descriptor and layout identity | `runtime/native_image/descriptor.rs`, compiler NativeIR managed-layout emission | native image, managed-layout, and artifact contract gates |
| Fail-closed transactional admission | `runtime/native_image/{descriptor,image,sealed,package_validation}.rs` | malformed image, descriptor mismatch, seal drift, and package admission tests |
| Managed owner and generation validation | `runtime/native_image/managed`, `runtime/native_boundary/resource.rs` | foreign-owner, stale-generation, wrong-kind, cleanup, and relocation tests |
| Strict foreign ownership mapping | C ABI and C++ binding-generator validation modules | unknown pointer ownership, borrowed lifetime, missing destructor, ambiguity, and exception rejection tests |
| Default unsafe-worker isolation | `runtime/native_boundary/{capability_sandbox,capability_wire,worker}.rs` | capability admission, bounded frame, worker crash, cancellation, and backpressure tests |
| Explicit work and memory budgets | descriptor codec limits, capability term/frame limits, specialization budgets, worker credits | boundary, overflow, depth/work, oversized frame, and credit tests |
| Adversarial positive/negative parity | native image, NativeBoundary, generated binding, sanitizer, mutation, and fault suites | `release-adversarial-corpus-check`, `release-mutation-check`, `release-fault-injection-check` |
| Narrow safety claims | native ABI specification and release documentation | `release-notes-accuracy-check`, `release-security-hardening-check` |

The 0.0.7 release must fail rather than defer any missing row in this table.
Trusted in-shard unsafe adapters, zero-copy foreign layouts, and relaxed
validation are not permitted as release shortcuts. ABI status remains
`current-pre-freeze` for the complete 0.0.7 line.

Gate: `make abi1-pre-freeze-check`.

## Scope Audit

The 2026-07-31 audit corrected both ordering and scope:

| Previous scope | Decision | Reason |
| --- | --- | --- |
| OTP test-suite migration implied GenServer/Supervisor completion | Expand and correct | Corpus disposition and low-level VM coverage do not prove executable Terlan stdlib service semantics. |
| Scheduler work followed concurrency proofs and HTTP claims | Move first | Fairness, reductions, and preemption are prerequisites for those claims. |
| Database proofs preceded dependency, migration, and SQL contracts | Reorder | Formal evidence must describe a stable executable contract. |
| Six domain-proof slices plus seven trace/dashboard slices | Consolidate to two outcomes | One semantic-closure gate and one deterministic release-evidence gate are clearer and cheaper to maintain. |
| Build-graph extraction repeated completed code-quality work | Shrink | CQ-3 already established thin binaries and canonical ownership; only measured artifact/incremental budgets remain. |
| Database orchestration and migrations were separate despite one lifecycle | Consolidate | They share admission, live-Postgres evidence, cleanup, and reporting. |
| Fifteen release micro-slices | Consolidate to five release outcomes | Installed use, compatibility, security, resilience, and sealing are the durable contracts. |
| Runtime inspector TUI | Defer to 0.0.9 | Useful tooling, but not required to prove the runtime or release boundary. |
| Durable process checkpoint/migration | Defer to 0.0.9 | Supervised restart is required now; durable state migration needs its own persistence design. |
| Mandatory HTTP/3 | Defer to 0.0.9 | 0.0.7 must report it honestly as unsupported/experimental; HTTP/1.1 and HTTP/2 are the production requirement. |
| General multi-service Docker orchestration | Shrink | 0.0.7 needs deterministic Postgres/serve dependencies, not a general container orchestrator. |

The release-completeness pass found four requirements that were present in
isolated gates but missing from the active dependency plan: installed
install/update/uninstall lifecycle, graceful service drain and process exit,
six-target candidate identity, and a final aggregate that activates every
required gate. They are now owned below. Deterministic fuzz/property seeds and
the generated known-limitation inventory are also explicit release evidence.

The Erlang/OTP migration ledger must use three distinct states:

1. `corpus_disposition_complete`: every imported fixture is ported, replaced by
   equivalent evidence, or retired with an implementation-specific reason;
2. `primitive_semantics_complete`: the VM hard primitives exercised by that
   corpus exist and execute on the direct-AOT path; and
3. `terlan_abstraction_complete`: high-level behavior such as GenServer and
   Supervisor is implemented in Terlan and passes Terlan-owned behavioral tests.

No row may use the first state as evidence for the other two.

## Dependency Order

Work proceeds from the first unchecked item downward. Do not select a later item
because it is smaller or related to recent edits.

1. Runtime scheduling, Terlan service abstractions, and supervision.
2. Database dependency, migration, and typed SQL contracts.
3. Typed HTTP configuration/observability, benchmark correctness, and TLS/HTTP/2.
4. Build-artifact containment and failed-edit-safe direct-AOT hot reload.
5. Formal semantic closure and deterministic proof evidence.
6. Installed-candidate validation, compatibility, security, resilience, and
   local sealing.

## Quality Enforcement Rule

A checkbox closes only after the implementation is usable through the intended
user path and its named gates pass. Every slice requires real behavior tests,
positive coverage, adversarial tests, stable typed failures, and documentation
when user-facing.

- Marker checks, symbol-existence checks, declaration-only tests, identity
  assertions, and dormant code are not completion evidence.
- Matrix, invariant, parser/renderer, and generated-value surfaces use
  table-driven or property-based tests where appropriate.
- Unsupported cases belong in a checked skipped/unsupported manifest with a
  stable reason and owner; silent skips are failures.
- Rust changes must pass `make rust-quality-check`, preserve the reviewed
  file-size limits, keep tests separate from implementation, and be inspected
  for code smells and duplicate abstractions.
- Direct-AOT behavior may not fall back to serialized VMIR, CoreIR evaluation,
  generated application Rust, or an interpreter.
- Release gates consume already-built candidate artifacts and may not publish,
  upload, tag, push, or require credentials.

## Phase 1: Runtime Semantics

- [x] Slice 40: close VM scheduler fairness, reductions, and preemption.
  - Scope: account for calls, receives, pattern matching, collections, timers,
    HTTP work, and NativeBoundary park/resume; add deterministic preemption
    points, starvation bounds, priority queues, cancellation, and replayable
    telemetry without changing result semantics.
  - Tests: exercise CPU-bound actors, recursion, mailbox/timer floods, parked
    processes, handler fanout, and cancellation at preemption points.
  - Gate: `make vm-scheduler-fairness-check`, followed by
    `make vm-multicore-release-check` and `make rust-quality-check`.
  - Acceptance: runnable actors do not starve beyond policy; all active AOT
    transitions are charged; identical seeded runs reproduce scheduling
    decisions; missing accounting or semantic drift fails with a diagnostic.

- [x] Slice 36: implement OTP-inspired service abstractions in Terlan and
  correct the OTP migration ledger.
  - Scope: implement GenServer, Task, Agent-like state cells, typed service
    loops, and policy wrappers as Terlan stdlib code over hard VM primitives.
    Remove or reject high-level `VmGenServer*`, `VmTask*`, `VmAgent*`, and
    equivalent framework intrinsics unless they are documented thin wrappers
    around spawn, identity, send/receive, timers, cancellation, links, or
    monitors.
  - Ledger correction: reclassify `gen_server_SUITE`, supervisor fixtures, and
    similar rows so corpus disposition remains complete but abstraction
    semantics remain pending until executable Terlan cases pass. Preserve the
    original test identities and source/expectation digests.
  - Tests: cover init success/failure, call/reply, cast, ordered state changes,
    timeout, terminate, crash/link/monitor behavior, stale replies, mailbox
    pressure, and unsupported framework magic.
  - Gate: `make vm-otp-abstractions-terlan-stdlib-check`, then
    `make terlan-vm-erl-suite-audit-check` and `make rust-quality-check`.
  - Acceptance: public service behavior executes from Terlan stdlib on direct
    AOT; Rust-only tests or compiler admission cannot satisfy the item; ledger
    states agree with executable evidence.

- [x] Slice 37: implement VM-owned supervision trees and restart semantics.
  - Scope: typed child specs, one-for-one/one-for-all/rest-for-one strategies,
    temporary/transient/permanent policies, intensity, backoff, escalation,
    ordered shutdown, and observable graph state for actors, HTTP services,
    and NativeBoundary workers.
  - Tests: cover crash loops, start failure, supervisor death, shutdown timeout,
    worker failure, pool exhaustion, cascades, and in-flight cancellation.
  - Gate: `make vm-supervision-restart-check`, then
    `make vm-otp-abstractions-terlan-stdlib-check` and
    `make rust-quality-check`.
  - Acceptance: every child failure has a deterministic typed restart or
    terminal outcome, no resource leaks, and no host-runtime supervision path.

## Phase 2: Database Runtime

- [x] Slices 46 and 45: close local dependency orchestration, migrations, and
  schema snapshots as one database lifecycle.
  - Scope: finish the existing typed Compose/Postgres path for `serve`, tests,
    migration commands, and SQL validation; support plan/start/health/reuse/logs/
    stop-owned while preserving external services. Complete live schema
    fingerprints, locking, replay, rollback, and protected rebuild validation.
    General-purpose multi-service orchestration is out of scope.
  - Tests: run deterministic live-Postgres migration/rebuild/snapshot replay and
    adversarial missing-Docker, health failure, lock contention, corrupt
    history/snapshot, remote destructive target, stale volume, and cleanup cases.
  - Gate: `make vm-dev-dependency-orchestration-check`, then
    `make vm-db-migration-command-check`, `make vm-postgres-runtime-check`, and
    `make rust-quality-check`.
  - Acceptance: required local dependencies become ready without sleeps or fail
    before runtime; migrations reproduce one schema fingerprint and cannot
    mutate protected targets or unmanaged services.

- [x] Slice 44: close typed SQL validation and row-shape contracts.
  - Scope: use maintained SQL/Postgres tooling to validate query kind,
    parameters, nullability, cardinality, row shape, transactions, and migration
    snapshot compatibility; do not hand-roll SQL parsing.
  - Tests: include live valid queries plus malformed SQL, injection-shaped
    interpolation, reordered parameters, duplicate aliases, schema drift,
    nullability/cardinality mismatch, and stale cache cases.
  - Gate: `make vm-sql-macro-validation-check`, then
    `make vm-postgres-runtime-check` and `make rust-quality-check`.
  - Acceptance: `sql!` lowers to a typed operation or fails before execution;
    runtime decode cannot cross an untyped row/parameter boundary.

## Phase 3: Production HTTP

- [x] Slices 34 and 35: close typed serve configuration and VM-owned
  observability.
  - Scope: deterministic config precedence and fingerprints; validate limits,
    protocols, TLS/ACME, assets, backpressure, telemetry, and shutdown before
    sockets open. Emit one versioned event/metric/trace schema spanning process,
    socket, request, scheduler, capability, TLS, and cleanup lifecycles. Include
    startup/readiness, signal handling, connection drain, shutdown deadline,
    final resource cleanup, and stable process exit status.
  - Tests: cover malformed/ambiguous config, unsafe public defaults, malformed
    trace context, event pressure, metric overflow, partial failures, repeated
    termination signals, forced drain timeout, and flush during shutdown.
    Production paths may not use ad hoc debug output.
  - Gate: `make vm-http-serve-config-check`.
  - Gate: `make vm-runtime-observability-check` after configuration, followed by
    `make vm-web-observability-check` and `make rust-quality-check`.
  - Acceptance: equivalent inputs yield one config fingerprint; every accepted
    request and typed failure is attributable through the shared schema; invalid
    config never reaches listener startup.

- [x] Slice 29: close trustworthy HTTP concurrency and performance baselines.
  - Scope: one request profile and metric schema for VM, Hyper, and Axum;
    identical parser/TLS paths where claims require comparability; fixed work,
    warmup, p50/p95/p99, throughput, errors, memory, confidence intervals, and
    runtime attribution. Treat AOT regressions against the accepted historical
    baseline as failures unless explicitly explained.
  - Tests: replay identical schedules across repeated runs and include malformed
    headers, slow clients, cancellation, backpressure, route bursts, and
    correctness assertions so throughput cannot be inflated by dropped work.
  - Prerequisite: the completed `make vm-http-concurrency-investigation-check`
    remains green.
  - Gate: `make vm-http-benchmark-comparability-check`,
    `make vm-http-runtime-attribution-check`, `make vm-http-soak-stability-check`,
    `make vm-http-vs-axum-check`, and `make rust-quality-check`.
  - Acceptance: reports share workload/config hashes and reject noncomparable or
    statistically unstable claims, unexplained >15% regressions, resource leaks,
    and non-monotonic scaling caused by runtime architecture.

- [x] Slices 32 and 33: close VM-owned TLS/ACME and HTTP/2 production behavior.
  - Scope: maintained crates own ACME, TLS, HTTP/2, and ALPN framing; the VM owns
    stream actors, scheduling, cancellation, backpressure, flow accounting,
    lifecycle, and cleanup. HTTP/3 is reported as experimental/unsupported for
    0.0.7 and is not a release requirement.
  - Tests: deterministic local ACME plus corrupt/expired cache, renewal races,
    challenge failure, ALPN selection, stream reset, flow exhaustion, slow
    frames, header/trailer limits, cancellation, and shutdown with live streams.
  - Gate: `make vm-http-acme-tls-production-check`.
  - Gate: `make vm-http-protocol-readiness-check` after TLS, followed by
    `make vm-http-soak-stability-check` and `make rust-quality-check`.
  - Acceptance: HTTP/1.1 and HTTP/2 use VM-owned stream lifecycle and typed
    cleanup; unsupported protocol features are explicit; no hand-rolled protocol
    or host-runtime scheduling semantics enter the server.

## Phase 4: Build And Developer Loop

- [x] Slice 90: enforce measured build-artifact and incremental-feedback budgets.
  - Scope: CQ-3 already closed thin binaries and canonical implementation
    ownership. Measure only canonical clean/warm/debug/release/coverage artifacts,
    invalidation, process starts, executable size, and feedback latency; remove
    the stale 68 GiB narrative and ratchet from a fresh reproducible baseline.
  - Tests: reject duplicate target roots, unaccounted artifact classes, stale or
    pre-build reports, cleanup-only evidence, and binaries reintroducing copied
    implementation trees.
  - Gate: add `make build-artifact-budget-check` after
    `make rust-build-graph-boundary-check`; also run
    `make package-build-artifact-isolation-check` and `make rust-quality-check`.
  - Acceptance: one canonical rebuild proves the checked disk and latency budget;
    operational cleanup alone cannot pass, and sustained improvements ratchet
    limits downward.

- [x] Slice 91: close failed-edit-safe direct-AOT developer hot reload.
  - Scope: a persistent compiler session coalesces changes, rebuilds proven
    dependents, validates a versioned native generation, and atomically admits
    it. Failed edits keep the prior generation serving; compatible state remains;
    incompatible state/resource changes reject or require explicit restart.
  - Tests: compatible handler/template edit, incompatible state edit, broken
    edit with uninterrupted old service, corrected activation, in-flight calls
    pinned to their generation, and adversarial partial/stale generation input.
  - Prerequisite: the completed `make watch-mode-hot-reload-check` remains
    green.
  - Gate: `make aot-developer-hot-reload-check`, followed by
    `make rust-quality-check`.
  - Acceptance: the direct-AOT path reloads without JIT/interpreter/generated
    Rust fallback, partial publication, silent state loss, or independent editor/
    browser/runtime polling protocols.

## Phase 5: Formal Evidence

- [x] Slices 15 through 20: close proof-visible Terlan semantic kernels.
  - Scope: consolidate template/route lowering, concurrency after Slices 40/36/
    37, collections, portable Wasm value boundaries, database type/effect
    boundaries after Slice 44, and release-critical std contracts. Prove only
    Terlan-owned semantics; external protocol, SQL-engine, and database-isolation
    behavior remains runtime/integration evidence.
  - Tests: each family has executable positive and rejection witnesses and at
    least one runtime-oracle fixture using the same typed contract and feature
    binding. Unsupported assumptions require an owned, expiring exception.
  - Gate: extend `make lean-proof-track-check` and
    `make lean-proof-track-runtime-check`; then run
    `make lean-proof-feature-binding-check` and `make rust-quality-check`.
  - Acceptance: every release-critical semantic claim maps to an executable
    theorem or explicit bounded exception; no parser-only, external-system, or
    placeholder theorem is counted as proof.

- [x] Slices 22 through 28: close deterministic proof evidence and release
  readiness without a dashboard subsystem.
  - Scope: emit one versioned canonical evidence manifest linking compiler input,
    feature/slice, proof IDs, runtime/std lanes, hashes, and candidate identity.
    Keep a small checked replay corpus and deterministic diff; do not store dated
    30-day histories or build a separate dashboard product.
  - Tests: valid replay plus missing proof, stale input, duplicate event,
    mismatched runtime lane, corrupt schema, candidate mismatch, and normalized
    cross-machine determinism cases.
  - Gate: `make release-artifacts-closeout-check`.
  - Gate: `make proof-readiness-release-mode-check`, followed by
    `make lean-proof-track-regression-check`, and
    `make lean-proof-track-pr-gate`.
  - Acceptance: one local command reproduces the minimal evidence and release
    mode consumes the same candidate; missing, stale, uncorrelated, or malformed
    proof evidence fails with a concise machine-readable diagnostic.

## Phase 6: Self-Validation

- [x] Close Python-free Terlan self-validation.
  - Scope: migrate every checked-in Python program under `tools/`, `scripts/`,
    and `std/scripts/` to runnable Terlan project scripts. Replace every Python
    invocation in Make, CI, documentation, and release orchestration. Python is
    migration inventory, not a supported validation tier or fallback.
  - Standard-library scope: when a Python tool needs an operation Terlan cannot
    express, add a typed portable Terlan API and its VM implementation first.
    This includes command-line arguments, environment and working-directory
    access, directory traversal and mutation, file metadata and atomic updates,
    binary IO, child-process execution and captured results, temporary
    workspaces, JSON/CSV/TOML and deterministic text processing, SHA-2 hashing,
    archives, platform identity, clocks, statistics, and bounded parallel work.
    Deterministic text processing must include an indexed or streaming Unicode
    scalar cursor; repository scanners may not materialize source text as
    `List[String]` one-character objects. Establish wall-time, allocation, and
    peak-managed-heap budgets with the shared-helper repository scan.
    The shared-helper gate uses indexed UTF-8 byte traversal, a 300-second
    wall-time limit, a 128 MiB process virtual-memory ceiling, and the VM's
    enforced 64 MiB actor-managed-heap ceiling; exceeding any limit fails the
    gate loudly.
    Unsafe OS handles and host-language values may not cross the API boundary.
  - Project-module scope: direct AOT must discover and link the executable
    implementation closure of imported project-local Terlan modules. Loading a
    `.typi` interface without the imported function body is insufficient;
    non-stdlib validation programs must be able to share typed Terlan modules
    without becoming single-file applications.
  - Executable-validation scope: external-package validators must compile the
    same complete direct-AOT closure they claim to validate. Static HTTP Router
    builders are compiler metadata and must be removed from executable images
    after route-plan extraction. Typed `CoreExpr::SqlQuery` expressions must
    lower through the VM-supervised Postgres capability protocol, reconstruct
    their declared `Result`/cardinality/row shape, and retain shard ownership,
    cancellation, and default-pool authority; a compile-only placeholder or
    interpreter fallback is not acceptable. This is a prerequisite for
    external application VM-contract gates are owned by their application repositories.
  - Bootstrap: a clean checkout may use Rust/Cargo to build `terlc`; all
    repository validation after that compiler bootstrap must execute through
    Terlan artifacts. Bootstrap cannot validate, regenerate, or silently repair
    source inputs through Python or a second implementation of Terlan rules.
  - Migration order: publish a complete versioned inventory and capability
    dependency graph; implement and test capabilities; migrate leaf checkers;
    migrate shared validation libraries and generators; migrate proof/release
    orchestration; then delete Python sources and remove Python installation
    from CI/release prerequisites.
  - Tests: exercise every new API with positive, error, permission, malformed
    input, cancellation/timeout, cleanup, deterministic ordering, and
    cross-machine normalization cases. For every migrated tool, replay its
    accepted and adversarial fixtures through the Terlan implementation and
    compare its stable diagnostics and artifact bytes before deleting Python.
  - Gate: add `make terlan-self-validation-inventory-check`,
    `make terlan-self-validation-capabilities-check`, and
    `make terlan-self-validation-check`; the aggregate must start from a clean
    temporary checkout state, build the compiler once, run all owned validation
    through Terlan, and reject any checked-in `.py`, Python executable lookup,
    Python Make/CI command, or Python fallback.
  - Acceptance: the repository contains no Python programs and requires no
    Python interpreter to validate, test, generate, package, or seal Terlan.
    A built Terlan toolchain validates its own compiler, VM, standard library,
    proof evidence, generated surfaces, and release candidate with deterministic
    results and typed machine-readable failures.

## Phase 7: Local Release Closeout

- [x] Slices 51, 65, and 66: validate installed examples, upgrades, and
  representative applications.
  - Scope: run curated examples, prior-project migrations, and nontrivial apps
    from clean temporary workspaces using only the installed candidate. Include
    first install, idempotent reinstall, upgrade from the prior supported
    layout, uninstall/cleanup, PATH collision handling, and exact version
    agreement between `terlc`, VM, native worker, stdlib, and editor artifacts,
    plus build/test/fmt/lint/package/VM behavior and safe migration previews.
  - Tests: workspace leakage, stale generated state, bad imports/packages/assets,
    mixed component versions, interrupted upgrade, stale files after uninstall,
    unsupported legacy behavior, silent migration writes, and non-default runtime
    dependencies must be detected.
  - Gate: `make release-example-projects-check`.
  - Gate: `make release-project-upgrade-matrix-check`.
  - Gate: `make release-reference-app-suite-check`.
  - Acceptance: representative clean installs work or fail with cataloged
    migration guidance; no fixture depends on checkout paths or hidden artifacts.

- [x] Slices 52, 53, and 60: close diagnostics, compatibility, migration
  guidance, and factual release notes.
  - Scope: generate stable text/JSON diagnostics and a public-surface diff;
    classify breaking changes and derive migration/release documentation only
    from installed examples, manifests, and gate evidence.
  - Tests: duplicate/undocumented IDs, text/JSON drift, unclassified API change,
    missing replacement guidance, unsupported release claim, and stale docs link.
  - Gate: `make release-diagnostic-catalog-check`.
  - Gate: `make release-compatibility-baseline-check`.
  - Gate: `make release-notes-accuracy-check`.
  - Acceptance: every public failure and compatibility change is cataloged and
    every release-note claim points to current candidate evidence.

- [x] Slices 54 and 55: close supply-chain provenance and security boundaries.
  - Scope: SBOM/checksum/license/dependency/unsafe inventories, reproducible
    candidate inputs, and default-deny VM, NativeBoundary, filesystem,
    environment, network, package, web, TLS, database, and installer controls.
  - Tests: undeclared dependencies, stale lockfile, unclassified unsafe/native
    code, capability bypass, stale handles, path traversal, oversized/slow input,
    ambient-secret leakage, and artifact-manifest mismatch.
  - Gate: `make release-supply-chain-provenance-check`.
  - Gate: `make release-security-hardening-check`.
  - Acceptance: shipped inputs have provenance and security-sensitive operations
    require explicit typed capabilities and failures; no hand-rolled protocol or
    unmanaged native artifact is admitted.

- [x] Slices 56, 57, 67, 68, and 69: close reproducible diagnostics,
  performance, adversarial, mutation, and fault evidence.
  - Scope: deterministic redacted support bundles; correctness-bearing compiler/
    VM/HTTP/package benchmarks; a minimized installed-candidate adversarial
    corpus with bounded property/fuzz seeds for parser, typechecker, image and
    worker protocols, HTTP input, and native descriptors; focused deterministic
    mutations; sanitizer evidence; and representative runtime/tooling fault
    injection. Reuse one report schema and fixture registry.
  - Tests: secret leakage, missing provenance, unstable/noncomparable benchmarks,
    stale or unowned corpus entries, surviving unclassified mutations, wrong
    recovery diagnostics, leaked resources, and silent skips.
  - Gate: `make release-support-bundle-check`.
  - Gate: `make release-performance-baseline-check`.
  - Gate: `make release-adversarial-corpus-check`.
  - Gate: `make release-mutation-check`.
  - Gate: `make release-fault-injection-check`.
  - Acceptance: evidence runs offline against the installed candidate, preserves
    correctness, explains host variance, and cannot hide failures through skips,
    raw errors, or missing cleanup.

- [x] Slices 58 and 59: seal and verify one candidate through a local staged
  distribution.
  - Scope: one deterministic attestation binds artifact/docs/editor/stdlib/VM/
    package hashes and all required reports to one candidate. Verify installation
    and rollback from an offline local mirror only; publishing is out of scope.
  - Tests: missing/stale reports, mixed candidate hashes, post-seal mutation,
    partial mirror, checksum/version mismatch, failed rollback, and any credential
    or source-checkout dependency.
  - Gate: `make release-readiness-attestation-check`.
  - Gate: `make release-staged-distribution-verification-check`.
  - Acceptance: local validation installs and exercises the exact attested
    candidate and safely rejects or rolls back incomplete/mixed staging; no
    external state change is required.

- [x] Slice 70: compose one offline proof of the complete 0.0.7 release.
  - Scope: use candidate-bound outcome reports for each durable release
    contract. Build and seal each candidate once; later gates consume that
    semantic evidence without rebuilding or mutation. Roadmap prose and
    completed-slice inventories are not executable validation inputs.
  - Scope: separate expensive evidence refresh from release preflight. A refresh
    produces candidate-bound reports and outputs; the candidate manifest records
    their hashes together with component, platform, toolchain, and environment
    evidence. Preflight validates and composes those sealed reports; it never
    reruns a completed gate merely because an
    unrelated later gate failed. The owning gate and its transitive dependants
    are the explicit refresh boundary. Automatic per-gate source dependency
    fingerprinting remains V8-1 work rather than a false 0.0.7 claim.
  - Scope: bind Linux/macOS/Windows on x86-64 and AArch64 attestations, runtime/
    image/NativeBoundary ABI versions, compiler/VM/worker/editor/stdlib versions,
    CPU and OS deployment baselines, installed smoke, documentation, language/
    std/JS/Wasm/native-package coverage, multicore controlled-host evidence,
    and every report produced by the preceding 17 outcomes to one candidate
    identity. Generate one stable supported/experimental/unsupported and known-
    limitation manifest from those reports.
  - Scope: define an explicit publishable-documentation manifest. Roadmaps,
    archives, raw baselines, scratch/research notes, and internal gate evidence
    may be source-controlled but must not enter installed or staged public docs;
    documentation hashes and links are computed from the staged manifest rather
    than the entire repository `docs/` tree.
  - Tests: delete or stale each required report in turn; inject a different
    candidate hash, unsupported platform claim, second build, post-seal
    mutation, missing controlled-host
    evidence, internal-roadmap leakage into public docs, and accidental network/
    publication command. Every case must fail with the responsible gate and
    reproduction command.
  - Gate: repair and run `make internal-docs-check` against the staged public
    documentation manifest.
  - Refresh command: `make release-evidence-refresh` after all preceding
    checklist items and their closeout gates pass. After a failure, rerun only
    the invalidated owner and its transitive dependants. Candidate-only drift is
    repaired with `make release-staged-distribution-verification-refresh`.
  - Gate: `make release-preflight` composes existing candidate-bound
    evidence and runs only final integration/sealing validation.
  - Acceptance: one offline composition command proves the complete release
    surface and emits one deterministic final summary. A no-op warm preflight
    executes no completed test suite or equivalent build. It cannot pass with a
    stale, missing, cross-candidate, or publication-
    dependent requirement, and it does not require a clean commit, a shared
    process lifetime, or remote state change.

## Validation

The release sequence has three semantic owners:

1. `make check` validates repository behavior and architecture once.
2. `make release-evidence-refresh` produces candidate-bound outcome evidence.
3. `make release-preflight` validates and composes that existing evidence
   without replaying completed suites.

`make release-check` is the version-neutral end-to-end entry point. It resolves
the release version from workspace metadata, refreshes the evidence, and then
runs the composition-only preflight.

This roadmap records intent and completion history. It is deliberately not parsed
by the build, and it does not duplicate the executable validation graph.
