# Terlan Erlang Emit Internals

This directory owns Erlang backend lowering and rendering modules. It is split
between syntax-output bridge lowering, CoreIR-oriented lowering helpers,
runtime helpers, and Erlang AST/render utilities.

## Responsibilities

- Lower Terlan module structures into Erlang backend structures.
- Render deterministic Erlang source.
- Keep syntax-bridge and CoreIR-lowering responsibilities separated.
- Provide focused backend tests for literals, control flow, imports,
  collections, records, macros, HTML, runtime helpers, and intrinsics.

## Public Surface

- Parent `emit.rs` exposes backend entry points.
- `syntax`: syntax-output bridge lowering modules.
- `core`: CoreIR-oriented Erlang lowering modules.
- `erl`: Erlang backend structure and rendering helpers.
- `erl::types`: Erlang type-expression render model.
- `erl::operators`: Erlang operator render identities.
- `util::type_specs`: Terlan type-text to Erlang spec lowering helpers.
- `runtime`: runtime helper emission.

## Core Model

The emit layer converts backend-neutral compiler structures into an Erlang
module representation before rendering text. It does not re-parse source and
does not own type checking.

The main flow is:

1. Accept module payload plus imported interfaces and artifacts.
2. Lower source declarations and expressions into backend representation.
3. Attach runtime helpers where needed.
4. Render Erlang text.

Important invariants:

- Lowering must be deterministic.
- Erlang escaping and atom/module naming must be centralized.
- Tests stay in adjacent `*_test.rs` files, not inline implementation blocks.

## Integration Points

- `syntax`: current broad source lowering bridge.
- `core`: formal CoreIR lowering path.
- `beam_process`: BEAM process helper emission.
- `terlan`: writes emitted source and optionally compiles it.

## Edge Cases

- Imported aliases and constructors must lower consistently across files.
- Mutable receiver methods must preserve Terlan rebinding semantics.
- HTML/template imports must not emit unsafe paths.

## Types And Interfaces

`ErlModule`
: Backend representation of one Erlang module.

`ErlType`
: Backend representation of one Erlang type expression.

`ErlBinaryOp` / `ErlUnaryOp`
: Backend operator identities rendered to Erlang operator tokens.

`lower_syntax_module_output`
: Syntax-output bridge lowering entry point.

`lower_core_*`
: CoreIR-oriented lowering helpers.

## Testing Notes

- Add emission regressions as adjacent `*_test.rs` modules.
- Runtime-sensitive tests should avoid assuming `erlc` is installed unless the
  test is specifically an Erlang integration test.
- Keep fixture output deterministic for exact comparisons.
