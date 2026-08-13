# Terlan Self-Validation

## Outcome

A compiler built from the Rust bootstrap validates the Terlan repository by
executing Terlan programs on the direct-AOT VM path. Python is not installed,
invoked, embedded, or retained as a fallback. The bootstrap builds `terlc`; it
does not implement a second copy of repository validation rules.

The canonical migration inventory is
[`TERLAN_SELF_VALIDATION_INVENTORY.tsv`](TERLAN_SELF_VALIDATION_INVENTORY.tsv).
The inventory now has no Python rows because every replacement has passed its
accepted and adversarial fixtures, all Make/CI consumers select the Terlan
replacement, and the Python sources have been deleted.

## Current Status

- `make terlan-self-validation-inventory-check` is a direct-AOT Terlan gate and
  proves that the checkout and canonical migration inventory contain no Python
  source rows while pruning build, VCS, and dependency artifact directories.
- `make terlan-self-validation-check` begins by materializing a clean,
  source-only temporary checkout, rejects Python source/cache artifacts and
  executable Python hooks, then runs the typed capability, generated std,
  documentation, browser-manifest, and release-promotion gates through the
  shared compiler bootstrap and sealed Terlan images.
- `make terlan-self-validation-capabilities-check` proves the typed argument,
  environment, file, exclusion-aware directory, and foundational string APIs.
- Safe `std.data.Json` operations now execute through a process-owned
  in-process NativeBoundary resource store. JSON handles retain generation and
  actor ownership checks without requiring an external `std` helper process.
- Safe `std.regex.Regex` operations now execute through the same process-owned
  in-process resource store; compiled handles retain owner and generation
  checks without delegating validation work to an external helper process.
- Direct AOT now lowers `String.length` and `String.byte_size`, so validation
  code can enforce exact Unicode-scalar and encoded-byte contracts.
- `std.system.Process.without_environment` now removes selected inherited
  variables before bounded child execution, while explicit typed overlays
  remain authoritative. The PyTorch CUDA gate uses it to keep nested Rust
  builds independent of repository lint flags.
- `std.system.Process.run_many` now executes an ordered command batch through
  a nominal typed request with a caller-selected concurrency bound. Per-command
  timeouts, output limits, typed failures, cancellation, and input-order
  results remain VM-owned.
- `scripts.self_validation.ChangelogPublicScope` is the first completed
  replacement: its Make consumer no longer invokes Python, its accepted and
  adversarial tests execute through Terlan, and the Python source is deleted.
- `scripts.self_validation.CudaPackageContractTest` now stages the complete
  sibling package closure, executes the external CUDA package from a temporary
  workspace, validates and seals its JSON evidence, and owns
  `make cuda-package-check`; the replaced repository Python checker is deleted.
- `scripts.self_validation.RoadmapLegacyRuntimeCleanupTest` now owns exact
  legacy-runtime reference classification, malformed/duplicate/stale-row
  rejection, and active-roadmap root selection; its Python checker and test are
  deleted.
- `scripts.self_validation.TerlanPytorchBaselinePolicyTest` now owns the
  CPU-required/CUDA-optional package policy, placeholder-symbol rejection,
  consumer/report validation, manifest hashing, and deterministic evidence;
  its output is byte-identical to the deleted Python checker.
- `scripts.self_validation.BuildInterfacesTest` now owns stdlib source
  selection, batched interface generation, bounded NativeBoundary metadata
  emission, and scratch-artifact cleanup. Its 212 generated artifacts are
  byte-identical to the deleted Python generator.
- `scripts.self_validation.JsBindingsDriftTest` now regenerates all 7,257
  pinned TypeScript binding artifacts in a temporary workspace and compares
  ordered generated/committed pairs through bounded batch reads. Its accepted
  repository result and stale-manifest diagnostic match the deleted Python
  checker.
- `scripts.self_validation.JsGeneratedReviewSurfaceTest` now validates binding
  and skipped-declaration schemas, all 3,707 reviewed skip rows, every required
  ES2015 collection module, safe generated paths, complete artifact presence,
  and provenance headers. Its accepted result and adversarial diagnostic match
  the deleted Python checker. Typed bulk JSON field projection keeps the check
  inside the VM without thousands of scalar NativeBoundary round trips.
- `scripts.self_validation.NativeArtifactsTest` now parses and deduplicates the
  Rust-backed manifest, regenerates all 18 NativeBoundary module artifacts,
  compares committed JSON/Rust bytes, compiles every generated Rust skeleton,
  and cleans its typed temporary workspace. Accepted and stale-artifact results
  match the deleted Python checker.
- `scripts.self_validation.SummaryDriftTest` now runs the Terlan-owned interface
  generator in an isolated workspace, compares all 190 regenerated `.typi` and
  dependency artifacts byte-for-byte, and rejects committed summaries that no
  ordinary std source regenerated while preserving generated-JS ownership.
  Accepted and stale-summary diagnostics match the deleted Python checker.
- `scripts.self_validation.RustBackedManifestTest` validates 147 manifest rows
  against compiler-owned NativeBoundary JSON instead of duplicating Terlan
  signature parsing. `RustBackedAdapterTest` separately proves the mapped Rust
  symbols and executable test owners. The migration exposed and repaired two
  missing JSON projection rows; accepted and missing-row diagnostics match the
  deleted Python checker.
- `scripts.self_validation.AllTerlanTestsVmInventoryTest` now owns the complete
  direct-AOT VM test-lane inventory through bounded tree pages. Its repository
  counts and exact forbidden OTP/BEAM diagnostics match the deleted Python
  checker without retaining the repository tree in one actor heap.
- `scripts.benchmarks.protocol.ProtocolBenchmarkTest` now owns workload-manifest
  validation, raw benchmark orchestration, nearest-rank statistics, baseline
  comparison and blessing, legacy Axum anchors, and deterministic JSON/TSV
  publication. All 72 normalized rows, legacy comparisons, metadata, and TSV
  output match the superseded Python implementation exactly.
- `scripts.self_validation.ReleaseVersionChannelTest` now owns canonical
  workspace/editor/installer/runtime version checks, release channel and tag
  admission, compiler-version probing, deterministic release reports, and the
  version-bump write path. Its repository report is field-for-field identical
  to the superseded Python checker, and write mode is exercised in an isolated
  typed temporary workspace.
- `std.scripts.ReleaseManifestTest` now owns the complete stdlib release
  manifest, generated documentation inventory, longest-prefix API ownership,
  and exact annotated-test coverage. Its accepted 83-module result matches the
  superseded Python checker; bounded filesystem phases avoid retaining
  generated documentation contents in the VM actor heap.
- `scripts.self_validation.CargoArtifactRetentionTest` now owns exact Cargo
  target cleanup, the shared incremental ceiling, deterministic retained-debug
  evidence, and explicit shared-debug maintenance. The new typed
  `std.io.Directory.tree_usage` capability measures logical and allocated tree
  bytes without following symbolic links through the direct-AOT VM path.
  Artifact-budget failures clean every profile created by the failed
  measurement before restoring compact evidence. Successful measurements
  immediately reclaim their isolated coverage tree, and the retention boundary
  also removes escaped test-owned `target/terlan-*` files or directories before
  the next validation lane while preserving shared debug and release outputs.
- `scripts.self_validation.VmBenchmarkFamilyPlanTest` now owns benchmark-family
  manifest, clue-table source references, VM-vs-OTP port-area mappings, and
  executable Make-target evidence. The headerless executable
  `MakeRecipeThinness.terls` prevents a typed Terlan gate from accumulating
  Python or shell control flow in its recipe.
- `docs_static_release_parity.Main` now extracts and verifies the installed
  release once, generates two deterministic documentation sites, validates
  3,101 module pages, 22,639 search entries, links, source-path isolation,
  installed hover/help/runtime behavior, and nine adversarial fixtures, then
  seals a size-bounded direct-AOT report. Its Make consumer runs one prebuilt
  Terlan image and the replaced Python checker is deleted.
- `scripts.self_validation.VmCliBridgeTest` now owns standalone image execution
  and compiler/VM bridge validation that previously used shell command
  substitution and branching. The migration also added direct-AOT lowering
  for composed `Bool.to_string` expressions.
- The test framework now treats `@benchmark` as a compiler-known executable
  case category. `terlc test --bench` reuses one admitted AOT image for warmup
  and measured samples, asserts every invocation, and reports native min,
  median, and p95 time independently from ordinary `@test` execution.
- Logic-bearing Make migration is active as a separate boundary audit. Typed
  Lean execution/reporting and JSON evidence membership have removed thirteen
  shell-control/text-pipeline candidates; 91 conservative candidates remain
  for classification and migration (including release orchestration, temporary
  workspaces, platform probes, and validation scans).
- `rust_quality.RustQuality` now owns Rust module-structure and file-headroom
  validation through one prebuilt direct-AOT image. Its indexed UTF-8 scanner
  preserves the Python oracle's accepted and adversarial diagnostics while a
  bounded source inventory holds the repository scan below 300 seconds, a
  128 MiB virtual-memory cap, and the VM's 64 MiB managed-heap limit. The three
  superseded Python programs and their test-hierarchy exemptions are deleted.
- `rust_quality.LintAllowances` and `rust_quality.WorkspacePolicy` now own the
  exact reviewed-allowance registry and centralized Cargo workspace policy.
  A direct byte-indexed Rust lexer ignores nested comments, literals, raw
  strings, and lifetimes; typed `std.data.Toml` supplies the manifest model.
  Accepted and adversarial policy surfaces run through the shared bounded AOT
  artifact, all source allowances have been removed, and the four superseded
  Python programs are deleted.
- `rust_quality.BuildGraphTimings` now owns CQ-3 clean and incremental Cargo
  measurements. It snapshots and restores nanosecond source timestamps,
  measures through the VM monotonic clock, bounds Cargo subprocesses, updates
  deterministic JSON, and removes only its typed isolated target tree.
- `rust_quality.ApiBoundary` now owns the Rust AST boundary report, exact
  string-error inventory, owner budgets, and 0.0.8 milestone enforcement. A
  bounded heterogeneous JSON projection streams the 22,000-row report without
  weakening the VM's managed-value conversion ceiling; Python and Terlan
  record modes produce byte-identical inventory and budget artifacts.
- No Python programs or executable Python hooks remain in the checked-out
  repository. The inventory and Make-recipe gates reject source or command
  regressions, and the former Battleship external-VM contract executes through
  the typed direct-AOT path.
- Controlled Rust timing provenance now uses
  `std.system.Platform.current_metrics()`. Linux `/proc` and sysfs parsing is a
  VM implementation detail; validation code receives a typed snapshot and
  fails closed when affinity, governor, memory, CPU, kernel, or load evidence
  is unavailable.

## Capability Dependency Graph

Migration follows this dependency order:

1. `std.system.Arguments`, `std.system.Environment`, and portable process exit
   establish script inputs and typed failures.
   The initial scalar `count/get` argument API is usable now; transporting
   managed aggregate results from VM capabilities remains an explicit
   prerequisite for an allocation-efficient `all()` convenience operation.
2. `std.io.Directory`, expanded `std.io.File`, VM-owned JSON handles, exact
   string byte measurement, and temporary-workspace APIs establish
   deterministic discovery, structured input, metadata, binary IO, atomic
   publication, permissions, symlink policy, and cleanup.
3. captured child-process execution, timeouts, cancellation, environment
   overlays, and bounded parallel execution establish orchestration without
   leaking OS handles into Terlan.
4. SHA-2, CSV/TSV, TOML, archives, platform identity, clocks, statistics, HTML,
   and deterministic diff APIs establish the reusable data plane.
5. leaf checkers and their tests migrate first, followed by shared generators,
   proof tooling, package/release orchestration, and performance drivers.
   Direct AOT must also load the implementation closure of imported
   project-local Terlan modules; interface-only import resolution is not enough
   for modular self-validation programs.
6. Make and GitHub Actions remove Python setup and invocations; a clean
   temporary-checkout gate rejects any remaining `.py` file or Python command.

The graph is intentionally one-way. A standard-library capability may use a
safe Rust implementation behind the compiler-owned NativeBoundary, but a
Terlan validation script cannot import a host-language object, call an
untyped escape hatch, or select a Python fallback.

## Migration Contract

Each replacement must:

- expose a zero-ambiguity command contract through typed Terlan APIs;
- sort filesystem and map-derived output explicitly;
- normalize paths, line endings, timestamps, and platform evidence where the
  artifact contract requires cross-machine identity;
- emit stable text diagnostics and structured diagnostics when consumed by
  another gate;
- fail closed on malformed input, missing files, duplicate records, stale
  hashes, partial writes, timeouts, cancellation, and cleanup failure;
- reproduce the prior accepted and adversarial fixture outcomes before the
  Python implementation is deleted; and
- execute as a normal AOT image through `terlc run script <name>` or an
  equivalent installed Terlan artifact.

Migration is complete only when the inventory has no pending rows, repository
search finds no checked-in Python program or Python invocation, and the
Python-free clean-checkout aggregate passes.
