# Terlan 0.0.8 Release Notes

Terlan 0.0.8 is a release-correction release. It preserves the compiler-owned,
direct-AOT runtime, fixed-owner multicore execution shards, and Executable
`.terls` scripts shipped in 0.0.7 while requiring the release commit itself to
be green across every applicable GitHub workflow.

Installed-candidate validation continues to cover the compiler, VM, native
worker, standard library, editor payloads, reference applications, and platform
artifacts. Linux compiler CI now installs bubblewrap and enables its required
unprivileged-user-namespace host prerequisite before capability-sandbox tests,
instead of allowing the environment to produce a false runtime failure. The
Linux launcher now closes arbitrary numeric inherited descriptors before
starting bubblewrap; this covers high descriptors exposed by hosted runners
rather than relying on the single-digit descriptor grammar of a POSIX shell.
Native platform setup is now shared by compiler and release matrices. Windows
uses target-matched vcpkg `libpq` and `pkgconf` packages and fails immediately
when any installer or tool smoke check fails.

Native platform attestations remain bound to the official repository, workflow
run, source commit, release version, and target. Each independent platform job
retains its own positive run-attempt identity, allowing a failed artifact upload
to be retried without rebuilding the five successful platforms. Evidence that
depends on temporal coupling, including performance and sanitizer pairing,
continues to require its stricter provenance policy.

Security analysis is repository-owned and limited to maintained GitHub Actions
and Rust source. The removed Python surface is no longer submitted to CodeQL.
The Foundations adapter updates to 5.9.2 and removes tracing and OTLP feature
exposure until its upstream dependency graph supports the patched OpenTelemetry
SDK; logging and metrics remain available.

The planned feature and accelerator work previously assigned to 0.0.8 now
belongs to the 0.0.9 roadmaps. Compatibility classifications and migration
guidance are recorded in `docs/release/COMPATIBILITY_0_0_8.json`.
