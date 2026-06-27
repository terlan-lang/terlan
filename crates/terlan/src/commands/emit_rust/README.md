# CLI Rust/Native Probe Module

This module owns the internal Rust/native neutrality probe. It is not a
release backend and is not exposed as an end-user `terlc` command yet.

Responsibilities:

- lower a narrow, backend-neutral `CoreModule` subset into Rust source
- compile the emitted Rust in tests when `rustc` is available
- verify CoreIR shapes are not accidentally Erlang- or JavaScript-specific
- keep Rust target details out of CoreIR, typechecking, parser, and validation

Current subset:

- function declarations from CoreIR
- public/private visibility as Rust `pub fn` / `fn`
- direct variable parameter clauses
- `Int`, `Bool`, `String`, and `Term`-like fallback signatures
- integer, boolean, string-like, variable, local call, function-value call,
  pipe-to-call, unary negation, and selected binary expressions
- selected primitive intrinsics, beginning with `core.string.contains`,
  `core.string.starts_with`, and `core.string.length`

Dependency rules:

- This module may emit Rust source text and use `rustc` in tests.
- It must not introduce Rust target types into `terlan_typeck` or CoreIR.
- Failures here should block only when they expose a backend-neutral CoreIR
  contract problem. They do not mean Rust/native is a supported 0.0.1 release
  backend.
