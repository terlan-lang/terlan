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

Generic inference keeps freshly instantiated variables separate from enclosing
bindings and other calls. An optional field such as `Some(price)` no longer
changes the inferred type of an unrelated sibling field. Regression coverage
includes reordered fields, earlier bindings, independent callbacks and shared
generic constraints, including genuine type errors that must remain rejected.
Substitution also flattens nested union members without reordering their layouts,
allowing fresh return-only generic results to adopt their enclosing context.

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
