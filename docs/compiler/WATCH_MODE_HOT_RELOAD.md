# Watch Mode And VM Hot-Reload Correctness

This document is the watch mode and VM hot-reload correctness contract for
0.0.7. The terlc watch command must define behavior for build, test, run,
serve, docs, package workspaces, formatter/lint checks, and VM hot reload using
the same incremental cache keys as clean builds.

## File Watching

File watching must normalize events, debounce deterministically, ignore
build/cache directories, detect package/lockfile/std changes, and avoid
triggering on generated artifacts unless they are declared watch inputs.

The file event contract explicitly includes ignore build/cache directories.

## VM Hot Reload

VM hot reload must preserve or reject process state according to a documented
compatibility rule:

- unchanged ABI/state shape may reload
- incompatible shape changes must fail with cataloged diagnostics
- stale processes must not observe mixed code versions

The contract explicitly covers preserve or reject process state, documented
compatibility rule, incompatible shape changes, stale processes, and mixed code
versions.

The hot reload contract explicitly defines a documented compatibility rule.

## Output Events

Watch output must include stable text/JSON events for start, change batch,
rebuild, diagnostic, reload, test result, support-bundle path, and terminal
failure.

The watch event stream explicitly includes terminal failure.

## Adversarial Cases

The adversarial matrix must include rapid file changes, rename/delete
sequences, package lockfile edits, generated file churn, stale source maps,
incompatible state shape reload, failing tests after reload, interrupted
rebuilds, and watcher path leakage.

The adversarial matrix explicitly includes rename/delete sequences and
interrupted rebuilds.

## Report Evidence

The executable gate persists watch-mode-hot-reload-report.json with:

- event sequences
- rebuild hashes
- cache hit/miss counts
- VM reload results
- diagnostics
- source-map parity
- support-bundle paths

The report is release evidence that watch mode and hot reload remain equivalent
to clean build/test/run for the same final workspace state while preserving VM
process-state safety.
