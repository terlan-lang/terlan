# Terlan VM ERTS Rust Quarantine

`terlan-vm/erts/rust` is quarantined migration reference material. It is not
part of the default compiler release graph, not release-blocking, and not wired
into `make check`.

The retained Make targets for this tree are Reference/history only. They may be
run manually while VM-owned code is ported into `crates/terlan`, but they must
use a temporary Cargo target directory outside the source tree. They must never
write build artifacts into `terlan-vm/erts/rust/target`.

When the retained inventory marks a crate as `migrate-first`, that row must
name a golden-owned evidence path outside `terlan-vm/erts/rust`. Deferred rows
may use `-` until they are either migrated into `crates/terlan` or deleted from
the retained tree.

This quarantine is temporary. The tree should be deleted after VM-owned code is
ported into golden-owned runtime modules and the migration inventory no longer
contains useful implementation evidence.
