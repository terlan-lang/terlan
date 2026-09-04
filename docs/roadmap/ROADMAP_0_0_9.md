# Terlan 0.0.9 Roadmap

Most of this roadmap begins after the 0.0.7 candidate is sealed. The validation
throughput foundation in V8-1 was pulled forward into 0.0.7 Slice 70 because a
monolithic same-run closeout proved operationally unreasonable; 0.0.9 retains
the broader tiering, cleanup, measurement, and ratcheting follow-through. Work
is selected in document order. Accelerator-specific work remains owned by
`ROADMAP_0_0_9_CUDA.md`; this file owns cross-cutting compiler, VM, tooling,
and release work.

## Validation Throughput And Evidence Reuse

- [ ] V8-1: make exhaustive validation fast without weakening release evidence.
  - Pulled-forward foundation: 0.0.7 now enters the canonical Rust suite once,
    reuses one union feature profile for quality/editor/benchmark test harnesses,
    executes freshness-checked prebuilt workspace tools instead of repeated
    `cargo run`, owns ignored evidence producers in that suite, measures clean
    artifacts before it, verifies sealed validator fingerprints afterward, and
    statically rejects Cargo-test replay in the release plan. Locked Tree-sitter
    dependencies and Node caches are now repository-owned and self-bootstrapping.
    The remaining requirements below own cross-tier orchestration, complete AOT
    and native-link reuse, lifecycle cleanup, and measured ratcheting.
  - Pulled-forward build optimization: the canonical Rust orchestrator now
    closes child stdin, bounds every Cargo phase, assigns every phase one
    deterministic tier, writes an atomic per-phase timing/outcome report, and
    fails when that report cannot be sealed. The
    shared typed-validator cache now has single-writer handoff, input and output
    content seals, mutation detection, stale-writer recovery, and failed-build
    cleanup. Validator children are bounded by a configurable thirty-minute
    deadline, and timeout or signal termination removes their partial state.
    Artifact-budget evidence is reused only when compilation inputs,
    policy, toolchain, profiles, and required prebuilt binaries still match;
    otherwise the clean six-lane measurement runs. Make no longer gives the
    Rust-suite owner a duplicate compiler-bootstrap prerequisite, and simple
    aggregate targets use dependency edges instead of recursive Make calls.
    Every ignored Rust evidence producer is now assigned to the checked-in
    six-tier inventory, with an explicit execution owner and isolation policy.
    Direct AOT unit and whole-image cache keys include the locked dependency
    graph, Cargo profile/features, target, codegen policy, and exact linker
    executable content; validation can turn an unexpected warm-cache miss into
    a loud error. Release evidence reuses the canonical Rust/build boundary,
    editor/docs parity builds one AOT image, and benchmark gates execute one
    prebuilt release binary. The release dry plan is machine-reported and
    ratcheted to zero `cargo run`, zero duplicate equivalent builds, at most six
    Cargo invocations, and at most seventeen typed-validator requests. Focused
    multicore gates now share the compiler bootstrap rather than repeating
    identical VM/compiler `cargo check` invocations, and the repository
    contract prevents direct Cargo recipes from bypassing that ownership. The
    typed test runner accepts multiple explicit source roots while preserving
    a dependency lifecycle per root; the standard capability lane now starts
    one compiler process for six roots instead of six processes. Its
    thirty-two-process release-plan ratchet prevents per-file startup
    duplication from returning. Purity metadata and Lean lane checks share the
    same mechanism, while focused binary, standard table, and property suites
    collapse 43 compiler startups into five explicit batches. The multi-root
    argument parser is isolated from execution ownership (171
    and 745 lines respectively), keeping both modules below the Rust headroom
    threshold instead of inventorying new structural debt. Cargo retention
    now caps regenerable debug incremental state at
    16 GiB, warns when the full debug tree exceeds 32 GiB, and preserves compiled
    dependencies and prebuilt tools during explicit incremental maintenance.
    The exhaustive entry point also rejects typed-validator partial state both
    before and after its reusable gate graph. The dry-plan report also seals the
    command-graph digest and ratchets its sixteen unique Terlan AOT builds;
    inherited warm-cycle flags cannot shrink the canonical plan it records.
    Typed-validator AOT recipes now default to verified incremental mode, and
    standalone source builds own the same checked-IR cache root as project
    builds. Checked cache identities include the exact frontend source closure
    and Cargo build policy. Receiver dispatch infers a fluent-chain receiver
    once and shares its type across primitive, local, and trait candidates;
    this reduced the 1,000-line repository validator's cold check from 7m14s
    to 1.63s. Embedded std interfaces are admitted from explicit imports,
    selected submodules, fully-qualified remote references, and the compiler
    prelude, with parsed summaries reused across every module in one build. The
    editor now applies the same import-closure rule to diagnostics, hover,
    completion, navigation, signatures, and inlay hints instead of parsing all
    1,636 packaged std summaries for each request. The focused hover/editor
    slice fell from 67.63s to 0.10s while retaining local, imported, and
    dependency-closure coverage. Auto-import discovery now text-filters the
    metadata-sealed catalog for one complete identifier and parses only the
    matching summaries; a cold lookup unique to one checked-in interface takes
    0.11s instead of about 25s, while mutations invalidate cached symbol
    results and local project summaries remain fresh. The Rust suite now
    compiles one union-feature
    library test harness and executes
    disjoint fast-unit and integration partitions from that binary, removing
    the second feature-profile build and cross-tier test replay. Cargo's JSON
    artifact identity now seals that exact harness after one bounded no-run
    build; nine Terlan partitions execute it directly, leaving only the separate
    workspace-support Cargo phase and reducing orchestrator Cargo launches from
    five to two. The measured
    libtest selection domains cover all 7,193 discovered test names (6,097
    library and 1,096 integration) with zero overlap; this removes the
    separate default-profile harness rebuild that took 2m26s after the same
    compiler source change. The remaining monolithic union harness is roughly
    421 MB; changing one adjacent test module still forced 55–124 seconds of
    recompilation and about 7 GB peak rustc memory in the 0.0.7 closeout tree.
    Split compiler implementation, reusable test support, and independently
    linked test tiers along stable crate boundaries so test-only edits do not
    recompile or relink all 7,101 library tests. Preserve direct sealed-harness
    execution, exact inventory ownership, and actionable Terlan line tables;
    an ambient host linker or globally disabled debug information is not an
    acceptable substitute. Cargo retention now also bounds hashed workspace
    test/tool executables to two settled generations per canonical stem with a
    five-minute writer grace period. Its first application reclaimed 29.88 GB
    of superseded executables, dep-info, and over-budget incremental state,
    reducing the repository target tree from 55 GB to 28 GB without deleting
    compiled dependencies or prebuilt tools. Compiler CI now performs workflow
    syntax validation in the canonical release-candidate job; the former
    standalone contract job rebuilt the compiler and repository validator only
    to repeat the same checked contract later in the canonical gate. Stable CI
    lanes now install only their exercised Rustfmt and Clippy components;
    developer coverage tools and the independently pinned nightly sanitizer
    source component no longer inflate every six-platform setup. `terlc test`
    now accepts repeated exact-name selectors and emits one native application
    for their union; the String capability gate consequently executes twelve
    exact contracts in one process without admitting unsupported whole-file
    tests. Pages caches the lockfile-bound Playwright Chromium payload across
    matching hosted runners while still installing and checking host browser
    dependencies on every deployment. Compiler and website implementation
    pushes no longer launch a second Docs CI compiler/site build after merge;
    pull requests and documentation-content pushes retain their focused docs
    validation. Cargo dev artifacts now use the same line-table-only debug
    policy as tests, retaining actionable backtraces while reducing compiler
    rlib, binary, and linker payloads. In the first clean profile rebuild,
    `target/debug/terlc` fell from 625,966,688 bytes to 277,986,168 bytes
    (55.6%) while preserving line attribution. Cold typed-validator misses now
    use two bounded build lanes after shared Rust tools are sealed; every lane
    retains its independent content seal, lock, timeout, and cleanup contract.
    The omnibus Rust-quality dispatcher still seals an approximately 205 MB
    AOT image: changing only its module-structure scanner required more than
    three minutes of cold regeneration. Split that dispatcher into stable,
    independently sealed validator families and reuse shared compiled support
    without increasing the seventeen-request or sixteen-AOT-build release-plan
    budgets. A split must preserve exact command ownership and must not replace
    cold work with duplicate compilation. The module-structure scan itself now
    rejects filenames globally but enters its comment/string-aware lexical
    pass only for files containing `include!`; its settled full Make gate is
    18.85 seconds and its direct repository scan is 15.97 seconds.
    Global Make serialization declarations are rejected because GNU Make 4.3
    applies them process-wide; the two genuinely ordered aggregates now use
    explicit local one-job submakes instead. CI workflow linting now pins Go
    1.25.0 and caches actionlint's module/build inputs, removing dependence on
    the hosted runner's ambient Go version. The accelerator CPU boundary scan
    now consumes the canonical repository validator and union-feature Rust
    suite; its standalone CI runner and duplicate full-crate Cargo check were
    removed without dropping the boundary or semantic tests. All non-lint CI
    jobs now opt out of Rustfmt and Clippy installation explicitly, including
    all six platform jobs, both sanitizer lanes, Docs, Pages, security audit,
    and controlled multicore evidence jobs; only the two exhaustive lint owners
    install those components. Registry protocol, archive, and CLI validation
    now consume the shared prebuilt compiler and canonical Rust harness instead
    of launching private Cargo builds or filtered test processes. Repository
    validation scans every shell check and rejects direct `cargo run`,
    `cargo test`, `cargo build`, or `cargo check` outside the two canonical
    freshness/exact-test wrappers. Cloud-bundle and Registry integration
    workspaces are unique per process and are removed on success or failure;
    converting those checks reclaimed about 5.3 GB of abandoned reproducible
    bundle and Registry output and prevents parallel validation lanes from
    sharing or retaining their work trees. Shared setup-action pushes also have
    one post-merge website compiler owner instead of launching both Docs and
    Pages builds. Release validation builds the compiler and VM before artifact
    measurement. Because that measurement intentionally runs `cargo clean`, it
    builds the quality tools and Rust boundary auditor once afterward; nested
    quality phases then verify those executables instead of rebuilding them.
    This keeps the honest six-invocation Cargo budget and prevents a clean
    release from depending on stale quality binaries. An absent prebuilt tool
    reports its exact binary instead of an opaque shell status. Artifact retention now runs only
    after the validation-owned builds whose state it measures, removing the
    overwritten pre-validation replay and lowering the Terlan process budget
    from 43 to 42, then multi-root capability, purity, and Lean execution
    lowered it to 34; shared Rust-backed standard-library and comprehension
    batches lowered it again to 32.
    Fixed-path standard-library and documentation fixtures clean
    themselves in their owning tests; release lifecycle boundaries reject and
    clean any residue left by an interrupted process.
    Publication is now a read-only consumer of candidate-bound evidence: an
    explicit refresh owns MC-9, AOT closeout, and release evidence once, while
    retries verify the sealed source identity and file digests without Cargo or
    test replay. The exhaustive hosted Compiler check is resolved back to the
    canonical workflow run before local evidence may reuse its Rust results.
    Five formerly external ignored capability/EPMD cases now execute directly
    from the union-feature harness. The remaining evidence refresh is capped at
    six distinct Cargo invocations and two isolated benchmark selectors, and a
    cross-worktree performance lease rejects a busy host before measurement.
    The MC-9 contract now executes after the canonical typed-validator
    bootstrap inside `release-candidate-check`; removing its preliminary CI
    step eliminates a compiler and platform-validator build that the clean
    artifact measurement immediately discarded.
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
