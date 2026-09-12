# Terlan process owner

Small shared process-lifetime primitives for the VM and build/test tools. This
crate has no compiler, scheduler, or capability-policy dependency.

`OwnedChild` retains the leader until Linux/macOS process-group cleanup, preventing
PID reuse from redirecting termination at a different job. Dropping the owner
terminates and reaps unfinished work. Other platforms currently own only the
direct child. Escaped sessions and Windows job objects remain separate work.
Cleanup also kills the retained direct PID: moving that child into a peer's group
cannot strand the owner in a wait, and the peer's group is never targeted.

Build tools use closed stdin, bounded stdout capture, and a single deadline for
execution and pipe drainage. VM capability code supplies its own typed framing,
cancellation, and policy on top of the same child owner.

Observed execution APIs report a PID only after a successful operating-system
spawn, while the child is still owned. If persisting that observation fails, the
owner terminates and reaps the child. This accounts for direct child launches;
it does not inventory uninstrumented nested compilers or test subprocesses.

`ProcessControl` optionally borrows a caller-owned cancellation flag. The same
flag covers pre-spawn admission, execution, and pipe drainage. The shared library
does not install signal handlers or impose a global VM cancellation policy.
