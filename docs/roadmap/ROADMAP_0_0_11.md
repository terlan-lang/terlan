# Terlan 0.0.11 executable roadmap

Status date: 2026-11-28

## Outcome

Terlan 0.0.11 makes compiler development sustainable. The release has two
co-equal primary tracks: bounded compilation performance and trustworthy
self-hosting. Feature work does not displace these tracks.

## P11-1: Bounded compiler planning

Observed pre-0.0.11 baselines range from 22 to 125 seconds and 0.50 to 3.13
GiB for equivalent focused/root gates, about 7 GiB for aggregate core
coverage, 9.3 GiB for multi-native planning, and 8.6 GiB for a cold compiler
build. An isolated generator-model coverage gate completes in 10.6 seconds
while the same crate connected to the monolithic compiler library takes 63
seconds. These measurements make dependency-graph breadth and native
planning—not test execution—the first optimization target.

A focused manifest/parser source check on the application dependency graph took
63.39 seconds and 454 MiB peak RSS on 2026-11-26. The checked source is not the
workload: this is a compiler package-discovery, interface-loading, or trait-
lookup performance defect. Source-only checking has its own stricter contract
and cannot inherit the native image budget below.

- [ ] Emit live phase progress and final timings for dependency discovery,
  interface loading, type checking, monomorphization, native planning,
  lowering, C/C++ compilation, linking, test discovery, instrumentation, and
  execution.
- [ ] Report peak and phase RSS, reachable declarations/providers, generated
  native units, helper reuse, cache hits, and attributed invalidation reasons.
- [ ] Plan native providers from the reachable program closure. Adding an
  unused provider must not change helpers, native units, planning time, or RSS.
- [ ] Reuse unchanged dependency interfaces, native helpers, monomorphized
  closures, and native images across `check`, focused `test`, and package gates.
- [x] Compile focused tests from their transitive import closure rather than the
  complete package graph.
- [ ] Keep ordinary local validation below the 10-minute goal and 15-minute
  hard ceiling. A phase silent for 10 seconds must emit progress.
- [x] Compile pre-commit compiler binaries, Clippy, and affected Rust tests
  under the single `validation-union` feature graph so Cargo reuses one
  library and test harness instead of rebuilding incompatible feature sets.
- [ ] Keep a warm focused source check below 5 seconds and 256 MiB peak RSS,
  and a cold focused source check below 15 seconds and 384 MiB peak RSS.
  Report discovered, loaded, reused, and invalidated interfaces plus attributed
  cache misses. Rechecking an unchanged source and dependency closure must not
  parse or reload unchanged interfaces.
- [ ] Keep a focused native regression below 30 seconds and 1 GiB peak RSS, and
  a broad release-candidate compiler gate below 5 minutes and 4 GiB peak RSS.
- [ ] Stop immediately with an attributed phase and resource diagnostic when a
  declared budget is exceeded.
- [x] Replace per-expression linear provider scans and import-set rebuilding
  with an indexed provider catalog and one import scope per function.
- [x] Emit a 10-second phase heartbeat during low-memory application planning
  so frontend, standard-library, reachability, pruning, and native-image stalls
  are distinguishable before a gate completes.

Focused test interface preparation now indexes source identities without
parsing their bodies, traverses only imports reachable from the selected test,
and prefers explicit `.terli` contracts over sibling implementations. The
deterministic closure algorithm lives in the existing planning boundary: its
complete 14-test crate gate finishes in 0.52 seconds at 195 MiB, and its 795
regions, 50 functions, and 447 lines have exact 100% coverage. Parsing and
interface publication remain thin compiler adapters; directory coverage still
prepares the complete package by design.

Acceptance uses matched focused/combined, narrow/broad dependency, cold/warm
cache, and reachable/unreachable native-provider fixtures. Test execution time
is reported separately from compiler planning time.

## P11-2: Self-hosting authority

- [ ] Continue the staged [self-hosting roadmap](ROADMAP_SELF_HOSTING.md) without
  stashing, sidelining, or weakening its parallel work.
- [ ] Make self-hosted compilation consume the same reachable-closure cache,
  phase progress, resource budgets, and regression corpus as the Rust frontend.
- [ ] Require deterministic Rust/self-hosted parser, type, Core IR, and backend
  differentials for every authority transferred in this release.
- [ ] Demonstrate that an unchanged self-hosted unit is compiled once per
  source/dependency identity, not once per command or test root.

## P11-3: Correctness and runtime prerequisites

The primary tracks must exercise and close these release-blocking contracts;
application-level workarounds do not satisfy them.

- [x] Add `Int.checked_mul_div` with widened, overflow-safe intermediate
  arithmetic and complete boundary coverage.
- [ ] Add atomic bounded file reads that enforce their allocation limit across
  concurrent file replacement.
- [ ] Add bounded, correlated, monitor-aware request/reply through a stable
  supervised service reference across physical process replacement.
- [ ] Preserve concrete generic receiver identity through field projection,
  chained receivers, imported trait results, and applied generic records.
- [ ] Lower exhaustive products of receiver-returned options, branch-local
  options, and valid recursive union constructors without semantic-ID drift,
  scalar/native-profile failure, or compiler stack overflow.
- [ ] Treat receiver-provider imports as used when method resolution consumes
  them.
- [x] Make a type import admit its provider's receiver and static operation
  namespace while keeping unqualified calls behind explicit function imports
  and constructors behind explicit value imports.
- [x] Include type-only imports and transitively imported non-test support
  modules in focused test compilation without adding test roots to package
  coverage ownership.
- [x] Preserve declared generic parameter order across source and package
  interfaces, including explicit type arguments whose result parameters do not
  occur in value arguments.
- [x] Refine output-only generic parameters from imported trait implementations
  during source inference and native specialization.
- [ ] Admit domain-neutral regressions for every item above before marking the
  corresponding compiler fix complete.

`Int.checked_mul_div` now computes through a signed 128-bit native intermediate,
truncates division toward zero, and returns `None` for zero divisors or final
results outside `Int`. Native ABI, Core intrinsic, bootstrap/direct JavaScript,
and 100% standard-library declaration coverage exercise signed, truncating,
widened, zero-divisor, and boundary-overflow cases.

Type import resolution now binds the local type name to its defining module,
including selected aliases, so both receiver calls and `Type.operation(...)`
calls share one import contract. Focused test context derives the active test
source root from its declared module identity and traverses type-only and
ordinary imports from that root. Explicit non-test support paths are admitted
as compilation roots in the same project without becoming runnable tests or
coverage owners. Domain-neutral positive, negative, provider-retention,
closure, and executable support-module regressions cover the contract.

Function schemes now retain generic variable identities in declaration order
instead of reconstructing order from first value use. Imported conformance
heads participate in output refinement, and NativeIR treats unresolved caller
parameters as unknown while matching a known generic provider. The exported
order fixture, four imported generic-trait execution fixtures, and focused Rust
regressions pass with inferred and explicit calls.

The compiler hook now activates one exact `validation-union` candidate before
source contracts and reuses it for every focused `terlan` Rust-test lane.
Clippy remains a release/CI contract instead of compiling the compiler graph a
second time during pre-commit. Hook contract tests enforce candidate-first
ordering and reject duplicate pre-commit Clippy builds.

## Release exit

0.0.11 is not ready while compiler planning remains unbounded or opaque or
while a claimed self-hosting authority lacks deterministic differential
evidence. Publication reuses successful exact-revision results and does not
rerun unchanged work.
