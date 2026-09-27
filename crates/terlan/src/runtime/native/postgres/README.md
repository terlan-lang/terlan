# Native Postgres Internals

This directory owns URL validation, libpq loading, and typed row decoding.
The legacy synchronous `Pool` API rejects live execution; it is not the source
runtime's database driver.

Command and test source calls use `runtime::vm::package_native_helper::postgres`
to project values
and `postgres_transport` to park the actor's continuation. An installed sibling
`terlan-native-worker` receives the Postgres capability and blocking-worker grant.
Its `native_worker::protocol::postgres` executor runs the maintained
`VmPostgresCommandClient` over the nonblocking libpq driver. The compiler-free
serve runtime never loads libpq itself.

Pools, transaction connections, and rows are actor-owned. JSON parameters cross
the process boundary as JSON text; worker handles never enter the VM's JSON
registry. Transaction callbacks execute in Terlan, with explicit worker calls
for begin, commit, and rollback. Terminal attempts invalidate connection handles.
Actor exit drops the worker, closes connections, and rolls back open transactions.

Each helper set admits at most 16 database worker owners and each actor may
allocate at most 4096 worker resources during its lifetime. One request per owner
is in flight. Transport calls have a 30-second deadline; the maintained client
also applies its database deadlines. Worker loss or a deadline can leave a write
outcome unknown. There is no automatic retry, worker restart, or handle reuse
following transport failure.

`make stdlib-postgres-worker-check` starts a disposable Docker database and runs
the public source API through the production compiler, VM, and external worker.
It fails when the fixture cannot start. Rust tests cover ownership, stale handles,
argument admission, cancellation, redaction, and representation checks.
