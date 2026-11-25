# Terlan 0.0.11 executable roadmap

Status date: 2026-11-26

## Outcome

Terlan 0.0.11 makes compiler development sustainable. The release has three
co-equal primary tracks: bounded compilation performance, type-directed
differential fuzzing, and trustworthy self-hosting. Feature work does not
displace these tracks.

## P11-1: Bounded compiler planning

Observed pre-0.0.11 baselines range from 22 to 125 seconds and 0.50 to 3.13
GiB for equivalent focused/root gates, about 7 GiB for aggregate core
coverage, 9.3 GiB for multi-native planning, and 8.6 GiB for a cold compiler
build. An isolated fuzz-core coverage gate now completes in 10.6 seconds while
the same crate connected to the monolithic compiler library takes 63 seconds.
These measurements make dependency-graph breadth and native planning—not test
execution—the first optimization target.

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
- [ ] Compile focused tests from their transitive import closure rather than the
  complete package graph.
- [ ] Keep ordinary local validation below the 10-minute goal and 15-minute
  hard ceiling. A phase silent for 10 seconds must emit progress.
- [ ] Keep a focused native regression below 30 seconds and 1 GiB peak RSS, and
  a broad release-candidate compiler gate below 5 minutes and 4 GiB peak RSS.
- [ ] Stop immediately with an attributed phase and resource diagnostic when a
  declared budget is exceeded.
- [x] Extract the type-fuzz generator model so its complete 100% coverage gate
  does not compile the monolithic compiler test harness.
- [x] Replace per-expression linear provider scans and import-set rebuilding
  with an indexed provider catalog and one import scope per function.
- [x] Emit a 10-second phase heartbeat during low-memory application planning
  so frontend, standard-library, reachability, pruning, and native-image stalls
  are distinguishable before a gate completes.

Acceptance uses matched focused/combined, narrow/broad dependency, cold/warm
cache, and reachable/unreachable native-provider fixtures. Test execution time
is reported separately from compiler planning time.

## P11-2: Type-directed differential fuzzing

- [ ] Complete [TF0–TF6](ROADMAP_TYPE_FUZZING.md).
- [ ] Run the bounded deterministic smoke corpus before expensive compiler
  gates and reuse its exact-revision result downstream.
- [ ] Generate dependency-breadth, native-provider reachability, cache-state,
  aggregate-root, and self-hosted/Rust-frontend differential cases.
- [ ] Admit every discovered compiler failure as a reduced, domain-neutral
  regression with neighboring generated mutations.

## P11-3: Self-hosting authority

- [ ] Continue the staged [self-hosting roadmap](ROADMAP_SELF_HOSTING.md) without
  stashing, sidelining, or weakening its parallel work.
- [ ] Make self-hosted compilation consume the same reachable-closure cache,
  phase progress, resource budgets, and fuzz corpus as the Rust frontend.
- [ ] Require deterministic Rust/self-hosted parser, type, Core IR, and backend
  differentials for every authority transferred in this release.
- [ ] Demonstrate that an unchanged self-hosted unit is compiled once per
  source/dependency identity, not once per command or test root.

## Release exit

0.0.11 is not ready while compiler planning remains unbounded or opaque, while
the bounded fuzz corpus has unexplained failures, or while a claimed
self-hosting authority lacks deterministic differential evidence. Publication
reuses successful exact-revision results and does not rerun unchanged work.
