# Terlan 0.0.8 Release Notes

Terlan 0.0.8 is a release-correction release. It preserves the compiler-owned,
direct-AOT runtime, fixed-owner multicore execution shards, and Executable
`.terls` scripts shipped in 0.0.7 while requiring the release commit itself to
be green across every applicable GitHub workflow.

Installed-candidate validation continues to cover the compiler, VM, native
worker, standard library, editor payloads, reference applications, and platform
artifacts. Linux compiler CI now installs the bubblewrap dependency required by
capability-sandbox tests instead of allowing the environment to produce a false
runtime failure.

Security analysis is repository-owned and limited to maintained GitHub Actions
and Rust source. The removed Python surface is no longer submitted to CodeQL.
The Foundations adapter updates to 5.9.2 and removes tracing and OTLP feature
exposure until its upstream dependency graph supports the patched OpenTelemetry
SDK; logging and metrics remain available.

The planned feature and accelerator work previously assigned to 0.0.8 now
belongs to the 0.0.9 roadmaps. Compatibility classifications and migration
guidance are recorded in `docs/release/COMPATIBILITY_0_0_8.json`.
