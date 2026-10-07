# Local commit validation

Run `bash scripts/git_hooks.sh install` once in each checkout. Installation sets
the checkout-local `core.hooksPath` to `.githooks`; existing custom hooks require
integration first. Git does not activate hooks automatically when cloning.
`bash scripts/git_hooks.sh check` runs the same checks without committing.

The hook reads the Git index, checks whitespace, and exports its exact tree into
an isolated clone at `target/git-hooks/source`. Its Cargo build cache lives at
`target/git-hooks/build` (or the supplied `CARGO_TARGET_DIR`). It never stashes,
resets, stages, or cleans the working repository. Partial staging, deleted and
renamed paths, and the alternate index used by `git commit --only` are supported.
Changes to the real index or HEAD during validation reject the commit. Checks
that alter tracked snapshot files also fail; fixes must be reviewed and staged.

Every nonempty commit checks Rust formatting, builds the staged compiler and VM,
and tests the hook policy and snapshot isolation. This requires Git, Bash, the
pinned Rust toolchain with rustfmt and Clippy, Make, and the compiler's native build dependencies.
The first run compiles a fresh cache and can take several minutes. Later runs
reuse it. The Terlan planner selects additional checks:

| Staged changes | Local checks |
| --- | --- |
| Rust source, compiler/grammar inputs, Cargo/toolchain/config, Make recipes | CI's `rust-clippy-check`: workspace binaries with default and all features, warnings denied |
| The above inputs, checked Rust quality reports, or their validators | Regenerate AST and Cargo metadata evidence; check API-boundary inventory and dependency-impact report without updating tracked files |
| Rust/compiler/grammar inputs, stdlib sources and summaries, or summary generators | Check native and JavaScript/DOM summaries against temporary regenerated output; requires Node/npm |
| Compiler syntax, grammar, shared syntax corpus | Syntax suite, phase-contract goldens, Tree-sitter checks |
| Other compiler source | Compiler suite and phase-contract goldens |
| Phase-contract fixtures alone | Exact phase-contract golden test |
| Other Rust source, Cargo manifests/lockfile, Rust toolchain/config | Workspace tests, including binary and integration targets |
| Root Makefile, isolated bootstrap Makefile, or hook runner | On Linux, run the real sandbox bootstrap integration suite; requires Bubblewrap and working user namespaces |
| Other `tests/fixtures/` inputs | Workspace tests; negative fixtures are validated by their owning tests |
| Terlan source and manifests | Build affected packages with their configured source roots; check standalone files; run changed or adjacent `*Test.terl` files |
| Editor source | Tree-sitter package, generation, and corpus checks; requires Node/npm |
| GitHub workflows/actions | Pinned actionlint, as in CI; requires Go |
| Shell scripts/hooks | Bash syntax checks |

The larger Rust fallback can take as long as the local workspace suite. The
focused suites do not prove all dependency impacts, all editor packages, every
Terlan consumer, cross-platform behavior, sanitizers, Lean proofs, or release
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
