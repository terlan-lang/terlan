# Local commit validation

Run `bash scripts/git_hooks.sh install` once in each checkout. Installation sets
the checkout-local `core.hooksPath` to `.githooks`; existing custom hooks require
integration first. Git does not activate hooks automatically when cloning.
`bash scripts/git_hooks.sh check` runs the same checks without committing.

For every failure fix, record the failing input and expected behavior, the
confirmed cause, why existing checks missed it, and the preventive change.
Add a behavioral regression at the owning boundary and run it locally before
pushing. Where practical, demonstrate that it fails against the broken behavior
and passes after the fix. Wire cheap checks before expensive compilation and
exercise producer/consumer boundaries with the real contracts. Updating an
expected value, regenerating a baseline, or retrying CI alone does not prevent
recurrence. Keep unresolved causes and validation limits explicit.

Build-plan limits have one definition in `RepositoryValidation.terls`. Make
delegates report admission to that typed validator; shell and Rust integration
fixtures verify dispatch and failure propagation without copying policy numbers.

Reviewed `.typi` API summaries remain source baselines. Their `.typi.deps`
companions are disposable compiler caches: standard-library loading, generated
binding review, and release payloads do not require them. `make
stdlib-build-interfaces` writes to `target/stdlib-interfaces` by default. To
refresh reviewed APIs deliberately, pass
`TERLAN_STDLIB_INTERFACE_OUTPUT="$PWD/std/summaries"` and review the semantic diff.
Normal incremental compilation still validates source, dependency and syntax
identities before reusing its own cache.

Commit messages must contain one nonempty line of at most 50 characters. Put
detailed explanations in the code, documentation, or pull request. Git's
`commit-msg` hook checks the message after it is collected; long subjects and
message bodies reject the commit. Editor-generated Git comments are ignored.
Run `bash scripts/git_commit_message_test.sh` to test this policy through real
commits in a temporary repository.

The hook reads the Git index, checks whitespace, and exports its exact tree into
an isolated clone at `target/git-hooks/source`. Its Cargo build cache lives at
`target/git-hooks/build` (or the supplied `CARGO_TARGET_DIR`). It never stashes,
resets, stages, or cleans the working repository. Partial staging, deleted and
renamed paths, and the alternate index used by `git commit --only` are supported.
Changes to the real index or HEAD during validation reject the commit. Checks
that alter tracked snapshot files also fail; fixes must be reviewed and staged.

The Cargo resolver cache defaults to `target/git-hooks/cargo-home`, keeping
unrelated packages in the global Cargo cache out of staged validation. An
explicit `CARGO_HOME` is honored; relative paths resolve from the working
repository root. The default cache persists between commits.

Every nonempty commit first runs `make rust-security-audit-check` against the
staged lockfile and the current RustSec database, with warnings denied. This
also catches newly published advisories when no dependencies were changed.
Install the pinned auditor with `cargo install cargo-audit --version 0.22.2 --locked`.
Missing tools, database download failures, and audit findings stop the commit
before compiler builds begin.

Every nonempty commit also generates fresh proof-lane and semantic-kernel reports
and checks the proof release baseline with the staged compiler before Terlan
formatting, browser tests, and the selected test lanes. The baseline binds
dependency inputs such as `Cargo.lock` as well as proof slices.
Changes require a reviewed baseline update; the hook never refreshes it.

A Make bootstrap regression checks that the artifact-budget validator survives
cleanup of disposable `_build` output before its later release gate runs.

Every nonempty commit builds the four canonical debug and release executables
and checks their combined sizes against `docs/quality/BUILD_ARTIFACT_BUDGET.json`.
The check uses the staged source and the hook's Cargo cache, rejects missing or
empty executables, and shares the release gate's size-budget calculation.
The full clean-build timing and artifact-footprint measurement remains a release gate.
After this size check, Terlan compilation, formatting, summary checks, and browser
fixture generation use the optimized compiler already built from the same staged
sources. Rust tests and semantic runtime oracles retain their debug builds.

The standard-library release manifest is checked against the staged sources,
including typed consumers for JavaScript bindings and opaque constructor types.

Every nonempty commit checks Rust formatting, builds the staged compiler, VM,
and service runtime, and tests the hook policy and snapshot isolation. It also
builds all JavaScript binding consumers with Terlan and executes the complete
Playwright suite in Chromium, Firefox, and WebKit. Module filters are cleared;
missing browsers, skipped cases, expected failures, retries, and incomplete
reports reject the commit. The paired
service runtime is required by executable service-package tests. This requires Git, Bash, the
pinned Rust toolchain with rustfmt and Clippy, Make, and the compiler's native build dependencies.
The first run compiles a fresh cache and can take several minutes. Later runs
reuse it. The Terlan planner selects additional checks:

Install the pinned browser test dependencies and browser binaries once:

```sh
npm --prefix tests/js ci --ignore-scripts
(cd tests/js && npx playwright install --with-deps chromium firefox webkit)
```

The browser matrix runs for every nonempty commit, including documentation-only
commits. Its compiler, sources, and generated artifacts come from the staged
snapshot. Reports and failure traces are under the snapshot's
`target/js-browser/` directory.
Selected source and report checks run before the browser matrix, so stale
generated reports fail before recompiling the browser fixture corpus.
The browser server selects a free port by default. Set `TERLAN_JS_TEST_PORT`
to request a specific port; `0` selects an available port automatically.

Typed CSS changes also regenerate-check all 752 specification-derived bindings,
check their types in bounded batches, execute native value tests and negative
fixtures, and build the browser stylesheet fixture through Lightning CSS.
The hook installs the pinned CSS and web toolchains for this gate. Standard-library
summary generation batches the independent CSS property modules separately.

| Staged changes | Local checks |
| --- | --- |
| Grammar/parser inputs, Lean proofs and metadata, compiler/runtime specifications, standard-library inputs, native-boundary sources, or proof validators | Check EBNF/LALRPOP/Lean fingerprints and replay the complete Lean proof track against the staged snapshot; requires the pinned Lean toolchain and, on Linux, Bubblewrap |
| Rust source, compiler/grammar inputs, Cargo/toolchain/config, Make recipes | CI's `rust-clippy-check`: workspace binaries with default and all features, warnings denied |
| The above inputs, checked Rust quality reports, or their validators | Enforce Rust size and test-layout rules; regenerate AST and Cargo metadata evidence; check API-boundary inventory and dependency-impact report without updating tracked files |
| Rust/compiler/grammar inputs, stdlib sources and summaries, or summary generators | Check native and JavaScript/DOM summaries against temporary regenerated output; requires Node/npm |
| Compiler syntax, grammar, shared syntax corpus | Syntax suite, phase-contract goldens, Tree-sitter checks |
| Other compiler source | Compiler suite and phase-contract goldens |
| Phase-contract fixtures alone | Exact phase-contract golden test |
| Other Rust source, Cargo manifests/lockfile, Rust toolchain/config | Workspace tests, including binary and integration targets |
| Root Makefile, isolated bootstrap Makefile, or hook runner | On Linux, run the real sandbox bootstrap integration suite; requires Bubblewrap and working user namespaces |
| Other `tests/fixtures/` inputs | Workspace tests; negative fixtures are validated by their owning tests |
| Typed CSS library, generator, bundler, or validation script | Complete typed CSS generation, type, native-value, rejection, and browser-build checks |
| Terlan source and manifests | Build affected packages with their configured source roots; check standalone files; run changed or adjacent `*Test.terl` files |
| Editor source | Tree-sitter package, generation, and corpus checks; requires Node/npm |
| GitHub workflows/actions | Pinned actionlint, as in CI; requires Go |
| Shell scripts/hooks | Bash syntax checks |

For Terlan files, Git plumbing compares the staged bytes with the staged
compiler's formatting of the same file at the commit's original base. Exact
matches still receive formatting and type checks, but do not launch changed-file
tests or external integration gates for layout-only edits. New files, invalid
originals, and any other source changes retain their selected tests. This does
not affect the unconditional complete Playwright matrix or other planned lanes.
Script entries outside a package's source roots use package-aware builds so
their imports resolve with the same configuration as normal compilation.
Run `TERLC=target/debug/terlc bash scripts/git_hooks_formatting_test.sh` to check
this comparison with the real formatter; the hook runs it with its staged compiler.

The larger Rust fallback can take as long as the local workspace suite. The
focused suites do not prove all dependency impacts, all editor packages, every
Terlan consumer, cross-platform behavior, sanitizers, or release
readiness. CI and the release gates remain authoritative. Hooks can be bypassed
by Git options, so they are local feedback rather than a publication attestation.

Before pushing a release candidate, run `make release-candidate-check` against
the candidate source on a supported local host, plus the applicable native and
sanitizer checks. A missing dependency, network failure, or sandbox restriction
is a blocked check, not a pass. Complete available local validation before
starting GitHub Actions; hosted platform jobs cover environments unavailable
locally.

`scripts/git_hooks_test.sh` tests Git isolation with small temporary repositories
under `target/`; CI runs it in source preflight. The planner tests run with
`terlc test scripts/git_hooks/src/git_hooks/PlanTest.terl` and are also executed by
every hook invocation. Bash owns only Git plumbing and command execution; the
selection policy lives in `src/git_hooks/Plan.terl`.

Missing tools and failed or empty Rust test selections stop the commit. Test
output identifies the failed gate. Review and stage golden updates explicitly;
the hook clears `TERLAN_UPDATE_PHASE_GOLDEN` and never accepts automatic repairs.
After a killed validation process, remove `target/git-hooks/lock` only after
confirming no other check is running. Concurrent invocations fail without
touching the active snapshot. Filenames containing newlines are rejected.

Rust report checks compile the staged validator once with the canonical optimized
compiler, then execute every report check with its paired VM.
