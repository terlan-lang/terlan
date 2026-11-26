# Terlan type-directed fuzzing executable roadmap

Status date: 2026-11-26

## Outcome

Terlan will continuously generate small, well-typed programs and prove that
every supported compiler path agrees on their meaning. The fuzzer must find
semantic interaction defects that declaration and line coverage cannot find,
reduce each failure to a stable source fixture, and make the reduced fixture a
permanent regression test.

This roadmap complements 100% declaration coverage. It does not replace unit,
integration, package-boundary, or application tests.

## Required command surface

The final user-facing commands are:

```text
terlc fuzz types --seed 17 --cases 1000 --max-depth 8
terlc fuzz replay target/fuzz/failures/<case-id>
terlc fuzz reduce target/fuzz/failures/<case-id>
terlc fuzz corpus check compiler/fuzz/type-corpus
```

All commands must be implementable by Terlan code once the required compiler
introspection APIs exist. A bounded bootstrap runner may initially call compiler
libraries directly, but it may not define a second parser, type system, or
backend model.

## Correctness oracles

Every generated case carries its expected type and effect facts. A case is a
failure when any applicable oracle disagrees:

1. The source parser rejects generated canonical source.
2. Source type checking disagrees with the generator's checked type.
3. Interface emission and re-import change public types or implementations.
4. Native lowering crashes, exhausts its budget, emits a generic unsupported
   diagnostic for supported source, or produces a different value.
5. Managed and native execution disagree.
6. JavaScript emission rejects a supported representation-preserving program
   or produces a different value.
7. Formatting changes meaning or is not idempotent.
8. A clean build and a warm cached build disagree.
9. An in-package program and the equivalent path-dependency program disagree.
10. Adding an unreachable native provider changes native planning work, peak
    RSS, generated units, or helper materialization.
11. Focused and aggregate roots produce different reachable native closures,
    or warm-cache and cold-cache runs disagree without an attributed
    invalidation reason.

Backend comparison uses a canonical typed value format. Unsupported target
features must have a precise capability diagnostic; they are not silently
discarded from the run.

## Generated program model

Generation starts from a requested result type and constructs expressions
backward through type-correct productions. The generator owns:

- lexical environments and binding identities;
- nominal, structural, union, product, function, and applied generic types;
- trait declarations and exact implementation evidence;
- constructor identities, patterns, and exhaustiveness witnesses;
- effect and suspension capabilities;
- backend capability requirements;
- value-size, recursion, declaration, and import budgets.

The initial type family is:

```text
Unit, Bool, Int, Float, String, Atom
Option[T], Result[T, E], List[T]
products, records, closed unions, nominal wrappers
functions, receiver methods, constructors, applied generics
```

Generation must deliberately compose features instead of sampling each feature
in isolation. Required interaction dimensions include:

- nested `if` and `case` joins;
- payload-free and payload-carrying union alternatives;
- lexical and pattern-bound values inside tuples, lists, and records;
- irrefutable and refutable destructuring;
- generic records containing applied generic records;
- constructor patterns across module and package boundaries;
- declaration-site trait implementations and qualified imports;
- suspension followed by typed continuation capture;
- recursive unions and recursive helpers;
- numeric operations and checked conversions;
- native-provider packages and JavaScript representation-preserving casts.

## Determinism and failure artifacts

A run is fully determined by the compiler revision, generator schema version,
seed, case index, target matrix, and budgets. Each failure directory contains:

```text
CaseTest.terl
case.toml
expected.json
observed.json
diagnostics.json
timings.json
resources.json
replay.terl
```

`case.toml` records all transitive inputs by content identity, never by an
absolute machine path. `replay.terl` reproduces the failure using the checked-in
Terlan runner. Replaying a recorded failure must not contact the network.

The reducer may only apply type-preserving transformations. It must retain the
same failure class and oracle disagreement while minimizing declarations,
imports, expression depth, type depth, and source bytes in that order.

## Resource policy

The default local smoke run has hard limits:

- 100 cases;
- 30 seconds wall time;
- 1 GiB peak RSS per compiler process;
- 2 seconds per case;
- 8 expression levels;
- 12 declarations;
- 3 modules;
- 1 path dependency.

The CI bounded run has hard limits:

- 1,000 cases or 5 minutes, whichever arrives first;
- 2 GiB peak RSS per compiler process;
- 5 seconds per case.

Budget exhaustion is a structured result with phase and resource evidence. A
stack overflow, abort, orphan process, or missing phase attribution is always a
failure. Long-running fuzzing is scheduled separately and cannot delay ordinary
commits or release publication.

## Milestones

### TF0: Contract and deterministic skeleton

- [x] Define the versioned case manifest and canonical typed-value format.
- [x] Implement deterministic seed splitting and case identifiers.
- [x] Implement bounded subprocess execution with CPU, wall, RSS, and output
  limits, transitive cancellation, and orphan detection.
- [x] Add replay tests proving identical inputs produce identical source,
  manifests, and outcomes.
- [x] Keep the deterministic generator model in an isolated workspace crate
  whose complete unit and coverage gate does not compile the compiler binary.

Acceptance:

```text
terlc fuzz types --seed 1 --cases 10 --oracle check
terlc fuzz replay target/fuzz/cases/<case-id>
```

Two independent runs must produce byte-identical case artifacts.

The generator and bounded executor crate has 100% line, function, and region
coverage: 6,771 regions, 327 functions, and 4,492 lines. Its 90-test execution
is subsecond, and its dependency-cold instrumented coverage gate finishes in
under six seconds. The executor measures the whole process group, enforces combined
stdout/stderr limits, and cleans up descendants after success, failure,
timeout, cancellation, or resource exhaustion. Reconnecting the crate to the
monolithic compiler library remains the expensive step and is tracked by P11-1
rather than hidden inside the fuzz gate.

### TF1: Typed scalar and control-flow generator

- [x] Generate scalar values, lexical bindings, products, records, `if`, and
  `case` from requested checked types.
- [x] Generate `Option`, `Result`, and closed unions with both nullary and
  payload alternatives.
- [x] Cover nested contextual joins with repeated `None`, `Some`, `Ok`, and
  `Err` alternatives.
- [x] Reject generator output that requires a cast not justified by the typed
  generation derivation.

Acceptance: 10,000 deterministic generated programs pass parse, type-check,
format-idempotence, and managed-execution oracles with zero unexplained skips.

Generation-scale acceptance is executable: 10,000 cases cover 17 initial
families in 0.05 seconds and preserve unique identities, checked result types,
canonical values, and the no-unjustified-cast invariant. Compiler and runtime
oracle acceptance remains open; generation success alone does not close TF1.
Every checked or native case now runs `terlc fmt --check` before its semantic
oracle. Formatting and execution share one wall, CPU, RSS, and output budget;
format failure stops before compilation, while native-planning probes remain
separate so formatter startup cannot contaminate compiler-work measurements.
All 17 generated families are byte-canonical under the current formatter.

### TF2: Native differential execution

- [x] Compile the TF1 corpus through native lowering and execution.
- [ ] Compare canonical results and structured failures with managed execution.
- [x] Record native phase timings, monomorphization counts, semantic type IDs,
  generated native units, compiler RSS, and cache disposition.
- [x] Separate planning, lowering, native compilation, linking, test execution,
  and coverage collection so immediate assertions cannot conceal multi-minute
  compiler planning.
- [x] Generate matched narrow/broad dependency graphs and prove unreachable
  native providers add no helpers, monomorphizations, planning work, or RSS.
- [x] Compare focused, combined-root, cold-cache, and warm-cache runs with
  explicit progress and cache-invalidation diagnostics.
- [x] Fail focused planning early when it exceeds 60 seconds or 1 GiB RSS, with
  the responsible phase, reachable declaration count, native-provider set,
  and cache-miss reasons in the failure evidence.
- [x] Provide a bounded deterministic reducer that preserves the oracle outcome
  and first structured diagnostic class.
- [ ] Reduce and check in every discovered failure before closing it.

Acceptance: the corpus includes permanent regressions for contextual union
identity, mixed-union aggregate results, recursive managed values, generic
record layout, suspension captures, and constructor-pattern identity.

The bounded native oracle is executable: generated cases contain independent
typed semantic assertions, native batches invoke `terlc test --timings`, and
successful runs retain measured CPU and RSS evidence. Its first three-family
probe immediately found a direct `Option[record]` return losing contextual
identity after scalar-case normalization. That minimized compiler regression
now passes with its neighboring generated families.

The corrected 17-family native batch passes in 1.7 seconds at 106.8 MiB peak
RSS and 1.2 CPU-seconds. This establishes the narrow-graph baseline; it does
not close the broad dependency-graph work, where small application suites have
previously required minutes and 1--2.3 GiB and full core coverage exceeded
7 GiB.

The bounded native oracle now also runs matched narrow and broad package graphs.
The broad graph declares an unreachable native provider whose helper is
deliberately absent, so successful execution proves the helper was not
materialized. Both graphs completed in 0.21 seconds; peak RSS changed from
105,216 KiB to 105,588 KiB. The comparison artifact records per-phase elapsed
time, generic specialization count, suspension profiles, lowered calls,
lowering-cache hits, native unit/function/continuation counts, and canonical
semantic-type identities with signed deltas. Both cold builds and verified
warm reuse now emit explicit cache outcomes from the owned test-image path.

The planning probe now executes six bounded profiles in one comparison:
focused, two-file combined-root, and broad-unreachable graphs, each cold and
warm. It records cache outcome, phase work, native image shape, CPU, RSS, and
wall-time deltas without rebuilding the compiler workspace. Unreachable native
providers are rejected if they add any structural compiler work, more than one
second of wall time, or more than 16 MiB RSS. Every profile must now report
phase timings, reachability, semantic-type inventory, and a cache disposition;
every warm profile must reuse a verified native image. The focused
implementation has 69 tests with exact 100% coverage across 4,791 regions, 232
functions, and 3,229 lines. A two-root profile must remain linear in structural
compiler work, below twice the focused wall time plus five seconds, and below
128 MiB of additive peak RSS. Its warm instrumented coverage gate finishes in
under three seconds; a dependency-cold instrumented build remains below 15
seconds. Focused and grouped test commands now emit independent `test.native-image`,
`test.execution`, and `test.coverage` timings around the existing native-IR,
module-unit, object, link, seal, and publication phases.

Rooted native lowering now releases the frontend-only binding and reference
inventory after pruning, before specialization and layout planning. The planning
comparison records those released counts and requires focused, combined, and
broad graphs to scale linearly in both dimensions.

`terlc fuzz reduce <case-dir>` now checks the original failure, probes at most
eight smaller same-family generated cases, retains only a matching structured
failure class, and publishes replayable observation and reduction artifacts.
Passing and infrastructure-only cases are rejected instead of being mislabeled
as compiler reductions.

### TF3: JavaScript differential execution

- [x] Select generated cases from an explicit JavaScript capability matrix.
- [ ] Compare canonical JavaScript results with managed results.
- [ ] Exercise lexical and pattern-bound payloads in tuples, options, lists,
  records, lambdas, and call arguments.
- [ ] Require precise diagnostics for genuinely unsupported target operations.

Acceptance: the corpus includes the lexical `Some(value)` identity-cast shape
that previously blocked `OperationTimeout.terl`, and its type-preserving
mutations all emit and execute correctly.

The JavaScript oracle stages generated modules by canonical module identity,
builds the complete supported batch through `js.shared`, and invokes every
exported semantic assertion under Node. Formatting, emission, and execution
share one wall, CPU, RSS, and output budget. The initial explicit matrix covers
14 scalar, lexical, product, record, list, control-flow, `Option`, and `Result`
families and completes in 5.97 seconds. Its first executions found two backend
defects rather than silently excluding the affected families: Terlan structural
equality had been emitted as JavaScript reference equality, and Boolean case
scrutinees could be emitted as undefined lexical bindings. Both now have
domain-neutral regressions. Canonical managed-result comparison, the complete
binding/payload matrix, and precise unsupported-operation diagnostics remain
open before TF3 can close.

### TF4: Modules, interfaces, traits, and packages

- [x] Generate multi-module selected and qualified imports.
- [x] Round-trip generated `.typi` interfaces and compare semantic facts.
- [x] Generate generic traits, declaration-site implementations, constructors,
  aliases, exhaustive patterns, and same-named types from distinct providers.
- [ ] Run equivalent in-workspace and external path-dependency layouts.

Acceptance: 5,000 package cases pass with no difference between source facts,
interface facts, qualified dispatch, generic bounds, or constructor coverage.

The first TF4 regression covers a generic field projection followed by an
imported receiver chain across four modules. Source checking completed in 0.56
seconds, while the pre-fix native path reproduced
`native_ir.generic_argument` in 0.22 seconds. Nominal field projection now
substitutes the enclosing generic arguments before receiver specialization; the
same fixture executes in 0.26 seconds.

Generator v2 adds a five-module family with selected constructor imports, a
qualified provider call, nested generic records, and the exact generic receiver
chain. Replay artifacts retain and verify every support source while v1 cases
retain their original family mapping. Its first bounded execution exposed two
previously hidden orchestration defects: generated projects used a Rust
manifest name and invalid current-directory source root, and canonical grouped
imports did not expand to sibling source providers. Generated cases now use a
valid `terlan.toml` with a confined `src` root, and focused closure preparation
expands selected imports to candidate child modules. The generated native case
prepares all four reachable providers and executes in 0.86 seconds; interface
closure discovery itself takes 75 milliseconds.

Every generated multi-module case now receives a second, isolated semantic
comparison after its source-backed oracle passes. The comparison copies only
the emitted `.typi` files and the unchanged consumer into a temporary project;
it copies no provider implementations, then requires the consumer to pass
`terlc check`. The first four-interface replay discovered zero source modules
and accepted the same generic receiver facts in 0.80 seconds. Each case records
the compared module identities, expected type, diagnostics, and bounded
resource evidence without proof hashes. Missing or malformed interfaces,
source/interface disagreement, deadlines, and publication failures are all
attributed to the originating case.

Generator v3 adds a four-module trait/package family. Two providers each
declare a distinct type named `Value`, a transparent `Payload` alias, the same
`Ready` and `Stopped` constructor names, and a declaration-site
`Score[Value]` implementation. The consumer aliases both constructor sets,
dispatches through a generic trait bound, and exhaustively matches both closed
event unions. Native execution passed in 0.42 seconds after preparing exactly
three reachable providers; an interface-only replay accepted the same facts in
1.24 seconds with no provider source. Generator v1 and v2 replay mappings
remain stable. The focused type-fuzz crate now passes 104 tests with exact 100%
region, function, and line coverage across 8,206 regions, 392 functions, and
5,412 lines.

### TF5: Suspension, actors, and native providers

- [ ] Generate typed suspension boundaries and continuation captures.
- [ ] Exercise ordinary calls, structured control flow, constructors, and
  union values before and after suspension.
- [ ] Generate cases using two compatible native providers in one package and
  verify helper routing and bounded binary transport.
- [ ] Verify worker, helper, and compiler processes are cancelled transitively.

Acceptance: no generated case may end in an unclassified suspension profile,
missing continuation capture, helper protocol mismatch, or orphan process.

### TF6: Continuous and release integration

- [ ] Run the deterministic smoke corpus before expensive compilation gates.
- [ ] Run only impacted target families for ordinary changes, using explicit
  source-to-feature ownership rather than filename guesses.
- [ ] Run the bounded cross-backend corpus on every compiler candidate.
- [ ] Run rotating and mutation-derived seeds on a schedule.
- [ ] Publish aggregate seed, feature-pair, oracle, failure, timing, RSS, cache,
  and reduction metrics without content hashes that require manual bumps.

Acceptance: a release candidate consumes the already-successful exact-revision
fuzz result. Publication does not rerun an unchanged fuzz corpus, and a changed
compiler cannot reuse evidence from another revision.

## Coverage model

The gate reports all of these independently:

- Rust line and branch coverage;
- Terlan declaration coverage;
- generated AST production coverage;
- type-constructor and nesting-depth coverage;
- ordered and unordered feature-pair coverage;
- backend and package-boundary coverage;
- diagnostic-class coverage;
- mutation score;
- unique reduced failures.

No single percentage is called "compiler coverage." CI fails when a required
dimension is missing, declines unexpectedly, or is reported without its
denominator.

## Regression admission rule

Every compiler bug found by an application or the fuzzer must add the smallest
domain-neutral fixture that reproduces its semantic shape. The fixture must:

1. fail on the parent revision for the expected reason;
2. pass source and every applicable backend after the fix;
3. contribute a seed or generator rule that produces neighboring cases;
4. run in the earliest bounded gate that could have detected the defect;
5. complete within its declared CPU, wall, RSS, and disk budgets.

A fix is incomplete when it only handles the reported source text, weakens the
type, erases a payload, adds a sentinel workaround, or excludes the feature
combination from generation.

## Definition of done

Type-directed fuzzing is operational when:

- all TF0-TF6 acceptance criteria are executable and green;
- a clean clone can replay every checked-in corpus failure offline;
- generated cases are well typed by construction rather than filtered after
  random syntax generation;
- native, managed, JavaScript, formatter, interface, cache, and package oracles
  use one canonical semantic expectation;
- failures shrink automatically into readable Terlan fixtures;
- ordinary CI remains bounded and release publication reuses exact-revision
  evidence instead of repeating unchanged work.
