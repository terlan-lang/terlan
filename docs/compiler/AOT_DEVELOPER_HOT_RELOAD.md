# Direct-AOT Developer Hot Reload

Terlan development serving uses one persistent compiler daemon and one
structured reload event stream. Filesystem notifications for Terlan source,
templates, styles, package inputs, and generated binding metadata are
deterministically coalesced before the daemon receives a source-event batch.
The browser, debugger source maps, editor diagnostics, runtime activation, and
VM TUI status are projections of that stream; they do not run independent
polling protocols.

The compiler session compares source checksums, invalidates changed modules and
proven import dependents, and reuses every other cached module. Candidate code
passes the formal frontend and Terlan-owned NativeIR-to-Cranelift object path.
There is no JIT, interpreter, generated application Rust, CoreIR runtime, or
VMIR runtime fallback.

Every candidate native image is compiled, linked, integrity checked, loaded,
and checked for its declared module, exports, public ABI, process-state shape,
capability imports, and native-resource contract before admission. Metadata is
written into an immutable generation directory. A single synced `active.json`
rename publishes the whole generation; readers reject missing, oversized,
escaping, stale, partial, malformed, or hash-mismatched generation input.

Admission replaces all prepared entries while holding the handler-cache write
lock and advances its epoch once. Existing request leases retain an `Arc` to
their exact loaded native generation, so in-flight calls and continuations
finish on old code while new calls select the replacement. Actor session state,
mailboxes, links, monitors, timers, supervision identity, and owned resources
live in the VM session service rather than the immutable code image and remain
intact across a compatible handler or template edit.

Parse, typecheck, code-generation, link, load, validation, and publication
failures leave the active pointer and runtime cache unchanged. Incompatible
public ABI, process-state, capability, or resource changes return stable
diagnostics and do not reset state. A developer may deliberately restart with
`TERLAN_SERVE_RESTART_INCOMPATIBLE=1`; restart is explicit rather than an
implicit consequence of an edit. A corrected edit can then activate normally.

Each session writes `.terlan/serve-aot/watch-mode-hot-reload-report.json` with
source-event batches, changed paths, invalidated modules, cache reuse,
compilation and activation timings, previous and candidate generation
identities, compatibility decisions, retained runtime state, failed-build
continuity, browser refresh, debugger, editor, and TUI events. The release gate
writes `target/quality/aot-developer-hot-reload-report.json` and executes the
native transaction test, including compatible edit, incompatible state edit,
broken edit, corrected activation, in-flight generation pinning, and
adversarial partial and stale generation rejection.

Release builds retain direct-call optimization and do not pay the development
reload registry indirection. The indirection and active-generation pointer are
development contracts only.
