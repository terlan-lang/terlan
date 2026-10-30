# Terlan 0.10.0 Release Notes

Terlan 0.10.0 adds single executable application deployment, broader TypeScript
interop, and typed CSS generation. Angular binding generation and lifecycle
helpers belong to Angular.ts's `integrations/terlan` package; the compiler
accepts its pinned declarations through `terlc bind typescript --manifest`.

## Before upgrading

Rebuild applications and native packages with the 0.10.0 toolchain. Handwritten
native methods use bodyless `@compiler.native` declarations. Rename `FixedArray`
to `Array`, use the owning module's public type path, and replace the old CSS
optional-property wrappers and getters with direct typed values.

Strings now support `${expression}` interpolation through `Show[T]`, including
custom implementations. Escape a literal interpolation opener as `\${` in
embedded shell, JavaScript, or other source text. Existing string capture
patterns retain their meaning.

Integer literals allow underscores between digits, including binary, octal and
hexadecimal literals. For example, `1_700_000_000_000_000_000` is an `Int`.
The formatter groups decimal integers in threes; floating-point literals keep
their existing syntax. Consecutive, leading and trailing numeric separators
are invalid.

Explicit constructors may return any declared result type. Checked construction
can return `Option[T]` or `Result[T, E]`; infallible construction can continue
to return `T`. No validation annotation or standard-library validation trait is
required.

Generic inference keeps freshly instantiated variables separate from enclosing
bindings and other calls. An optional field such as `Some(value)` no longer
changes the inferred type of an unrelated sibling field. Regression coverage
includes reordered fields, earlier bindings, independent callbacks and shared
generic constraints, including genuine type errors that must remain rejected.
Substitution also flattens nested union members without reordering their layouts,
allowing fresh return-only generic results to adopt their enclosing context.

Signed constants and valued-union arms retain their nominal types across
package summaries. Receiver calls directly on qualified union constants use
ordinary receiver dispatch. Constructor coverage and native matching share
canonical atom tags, including multiword names, acronyms and digit boundaries.
Missing variants and branches covered only by guards remain rejected.
Renamed imported constructors retain their provider identities during coverage,
payload refinement and native matching, including same-named constructors from
different modules.

Native helpers support chunked owned-buffer transfers up to 16 MiB while
keeping physical frames bounded to 1 MiB. Regenerate C/C++ helpers to receive
large arguments; the runtime can stream bounded legacy binary replies.

`std.vm.Supervisor` admits typed ordinary processes into the VM supervision
graph, exposes replacement identities and restart history, and enforces restart,
backoff, and shutdown policies. Optional file-backed snapshot/event hooks let a
recovery entry restore a logical child's state after a crash. Nine compiled
Terlan regressions exercise these operations; `Process.spawn` remains
unsupervised. See [VM source supervision](../runtime/VM_SOURCE_SUPERVISION.md).

Native applications carry their image and VM in the executable. External service
dependencies retain their own deployment requirements. This does not freeze the
runtime ABI or claim completion of compiler self-hosting.

The classified changes from 0.0.9 and their migration guidance are recorded in
`COMPATIBILITY_0_10_0.json`.

## Validation requirements

Standard-library dependency cache files (`.typi.deps`) are no longer required
source artifacts or release payloads. The compiler and editor derive dependency
closures from interface declarations. Reviewed `.typi` API summaries remain
checked against regenerated declarations; incremental compiler caches retain
their source, dependency and syntax checks. Standard-library generation writes
to `target/stdlib-interfaces` by default, and generated JavaScript manifests
exclude disposable cache hashes.

`terlc doctor` accepts current interface caches in `.terlan` and
`_build/.terlan`, as well as reviewed API declarations without cache sidecars.
It diagnoses malformed summaries and mismatching compiler metadata rather than
treating every generated interface as stale.

Build-plan generation and report admission share one typed budget definition,
removing duplicated Makefile and test-fixture limits. Regression coverage checks
stale or absent sidecars, changed interfaces, malformed reports, changed limits,
and producer/consumer failure propagation.

Installed-candidate validation must cover the direct-AOT runtime, fixed-owner
multicore execution shards, the compiler and native worker, and package and
service deployment. Executable `.terls` scripts remain part of that validation.
Platform support is determined by the current candidate's successful artifact
matrix and smoke tests. Publication requires fresh candidate-bound evidence;
these notes do not substitute for it.

Linux binary requirements: x86-64 requires glibc 2.35 or newer (Ubuntu 22.04
baseline); AArch64 requires glibc 2.39 or newer (Ubuntu 24.04 baseline).
These are dynamic GNU/Linux binaries, not musl/Alpine binaries. Native library
requirements such as libpq remain applicable. Release packaging rejects ELF
symbol requirements newer than the declared baseline. The installer checks
`minimum_glibc` in the verified release metadata and starts each staged
executable before replacing an existing installation, so missing native
libraries also produce an error before replacement.

Zero-argument source constructor patterns now retain their provider identity and
match literal atom or singleton alias bodies in both mixed and all-atom unions,
including renamed imports. Alias bodies resolve before constructor identities
are collected, so matching no longer depends on alias-rewrite pass order.
Generic compiled regressions cover local and imported forms.

Importing a cached package summary now loads its reachable standard-library
dependencies before checking constructor coverage. This keeps imported singleton
atom tags consistent with directory checking; generic regressions cover aliased
tags, local matches, interface round-tripping, and compiled package tests.

Path-dependency imports preserve provider-owned type identities through generic
argument checking, imported signature expansion, and CoreIR lowering. Same-named
types from unrelated providers no longer substitute for process entries or
closed-union results. External package regressions execute cold and warm imports
and verify that a genuine provider mismatch remains rejected.

Constructor physical tags now remain self-contained in serialized CoreIR, so
pattern-only imports survive package caching and dead-code pruning. Native
lowering supports imported tuple-constructor patterns inside suspending helpers,
including process entries that call the helper after pure structured cases.
Cold and warm path-package regressions exercise both decomposition shapes.

JavaScript Array conversion accepts contextually typed empty list literals.
The shipped consumer is compiled and executed in regression coverage. Browser
validation stops on consumer compilation failures after saving diagnostics.

Controlled executable launches now retry the operating system’s transient
write-held executable error within the existing timeout and cancellation limit.
Regression tests cover successful release, timeout, and cancellation.
