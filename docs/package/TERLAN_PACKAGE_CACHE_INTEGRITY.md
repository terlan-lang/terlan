# Terlan Package Cache Integrity Contract

Terlan package cache behavior must be deterministic, checksum-backed, and safe
for builds, editor workflows, package CLI workflows, and VM runtime loading.
Cache corruption must produce cataloged diagnostics instead of fallback behavior.

The package cache layout records:

- archives
- expanded sources
- generated bindings
- native artifacts
- docs summaries
- registry snapshots
- lockfile metadata
- temporary extraction state

Cache keys are content-addressed or checksum-verified. Cache keys include
target constraints, capabilities, and native-artifact dimensions when package
outputs differ by target.

The package cache command surface is:

- `terlc package cache verify`
- `terlc package cache clean --check`
- `terlc package cache prune --dry-run`

Cache verification validates cache state without mutating files unless the user
requests an explicit write action. Clean and prune checks must never remove
live dependencies, follow unsafe paths, or silently fall back to workspace paths.

Corrupted, partial, stale, target-mismatched, yanked, and provenance-mismatched
cache entries fail with cataloged diagnostics.

The adversarial package cache corpus covers corrupted archives,
partial extraction, stale native artifacts, stale generated bindings,
target-mismatched cache entries, cache poisoning,
symlink/path traversal attempts, concurrent cache writes, and
clean/prune commands deleting live dependencies.

The package cache integrity report is `package-cache-integrity-report.json`. It
records:

- cache fixture paths
- verified entries
- rejected entries
- prune plan
- diagnostics
- checksum coverage
- concurrency behavior
