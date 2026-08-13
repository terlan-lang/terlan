# Terlan 0.0.7 Release Notes

Terlan 0.0.7 makes the compiler-owned, direct-AOT `terlan-vm` path the native
default for build, run, and test. The installed artifact matrix proves that the
compiler, VM, native worker, standard library, and editor payload are packaged
under one candidate version. Evidence:
`target/quality/vm-release-artifact-matrix-report.json`.

The runtime now uses fixed-owner multicore execution shards: actor heaps,
continuations, timers, and scheduler queues remain shard-local, while immutable
AOT images are shared. Evidence:
`target/quality/vm-multicore-release-closeout.json` and
`docs/runtime/TVM_MULTICORE_CONCURRENCY_CONTRACT.md`.

Installed-candidate validation covers curated language examples, deterministic
project initialization, the supported 0.0.6 project layout, migration
diagnostics, and seven representative multi-module applications. Evidence:
`target/quality/release-example-projects-report.json`,
`target/quality/release-project-upgrade-matrix-report.json`, and
`target/quality/release-reference-app-suite-report.json`.

Executable `.terls` scripts can drive AOT validation without an application
`main` function and propagate assertion failures to their caller. Evidence:
`docs/language/SCRIPTS.md`.

Compatibility notes and replacement guidance are maintained in
`docs/release/COMPATIBILITY_0_0_7.json`. In particular, remove legacy
Erlang/OTP target options and use `terlc migrate pattern-head` before rewriting
rejected reverse-alias function-head patterns.
