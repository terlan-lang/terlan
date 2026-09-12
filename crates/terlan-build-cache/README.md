# Rust build-cache maintenance

Compiler-independent Linux bootstrap tooling for Rust 1.96.0 incremental caches.
It has no dependency on the Terlan compiler or VM and is not a shipped product.
Native file locking is required before those products can be rebuilt, which is
why this small bootstrap is Rust rather than another shell or AOT script.

From the repository root, build it once with `make rust-build-cache-bootstrap`.
On Linux this shares one Cargo invocation with the small build-process owner,
which is installed separately before the compiler is rebuilt. It does not build
the Terlan compiler or VM.
`make rust-incremental-cache-audit` is read-only; the explicit
`make rust-incremental-cache-prune` retires redundant finalized sessions older
than five minutes. Neither command builds the tool implicitly.

The only supported input is `target/debug/incremental`. Each crate/flavor keeps
its newest finalized session. Working sessions, missing locks, and busy rustc
shared/exclusive locks prohibit retirement. Protected data is never deleted to
satisfy the 64 GiB / 1,024-session budget. Exit 2 means the budget was exceeded or
could not be verified, including active unmeasured sessions or pending retirement.
Exit 1 means invalid arguments, unsupported layout/platform, or an I/O failure.

Retirement uses a separate maintenance lease, rustc's existing session lease,
and rename into `target/debug/.terlan-incremental-retired-v1`. A later prune
finishes recognized partial unlinks after a process crash. Unknown files,
symlinks, nested directories, and unsupported Rust cache headers fail closed.
The policy assumes a trusted local build directory, not adversarial ancestor
replacement or filesystem power-loss durability. Rust lock files are left to
rustc; they are never recreated or removed as session-deletion authority.

Reported allocated bytes count each file inode once inside the namespace.
They are not a guarantee of filesystem reclamation: hardlinks outside the cache
can retain storage. This does not bound Cargo artifacts, registries, other
profiles/targets, working sessions' age, or the newest retained generations' age.
Tests are owned by the canonical workspace-support tier, including real rustc
object reuse and Linux process-kill recovery.
