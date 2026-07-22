# Terlan VM External Repository Boundary

Status: 0.0.7 baseline contract.

The old external `terlan-vm` repository is temporary history or migration
source only. It is not an active compiler dependency, not a public runtime
distribution, and not required by default release checks.

Allowed uses:

- Reference-only migration evidence for extracting Terlan-owned runtime tests.
- Ambitious is a reference checklist only for process, registry,
  supervision, presence, and distribution behavior.
- Explicit opt-in OTP compatibility comparison gates that are outside default
  release flow.
- Historical tests that prove old cross-repository dependency shapes are no
  longer accepted as active product structure.

Disallowed uses:

- Default `make check`, `make test`, or `make test-release` dependencies on a
  sibling VM checkout.
- Cargo workspace membership for an external VM crate.
- Ambitious or another third-party OTP-like runtime becoming a core
  dependency; Ambitious is not a core dependency.
- Runtime execution paths that shell into the old repository.
- Package metadata that treats the old repository as a required dependency.

The active VM implementation lives inside `crates/terlan` and is shipped
through the same release train as the compiler. `terlc` and `terlan-vm` use the
same workspace package version, the same compiler crate manifest, and the same
release artifact path.

Terlan VM owns scheduling, process state, mailboxes, supervision, registries,
runtime inspection, and distribution semantics. External projects may inform
tests and naming, but they must not define the VM execution model.

## Gate

The contract is guarded by:

```bash
make terlan-vm-external-repo-boundary-check
```
