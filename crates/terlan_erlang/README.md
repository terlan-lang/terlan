# [terlan_erlang] Internals

This package owns Erlang backend emission for Terlan. `src/lib.rs` is the
crate surface, `src/emit.rs` is the emitter entry-point and orchestration layer,
`src/emit/` contains the larger lowering/rendering submodules, and
`src/pretty.rs` is a minimal formatting shim.

## Responsibilities

- Convert Terlan modules into Erlang source strings.
- Emit interface-aware code when external constructor/type data is available.
- Generate type specs, exports, records, and constructor helper functions.
- Provide a minimal pretty-printing hook for future backend formatting.

## Public Surface

- `try_emit_syntax_module_output_to_erlang`: strict formal compiler-path emitter
  entry point for compiler-facing syntax output.
- `try_emit_syntax_module_output_to_erlang_with_interfaces`: strict syntax-output
  emitter with interface map context.
- `try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown`:
  strict syntax-output emitter used by the CLI emit path.
- `try_emit_core_module_to_erlang_with_syntax_bridge`: transitional
  CoreIR-gated emitter used by the formal CLI emit path while CoreIR expression
  payloads are still being expanded.
- `try_emit_syntax_struct_headers_to_hrl`: strict syntax-output header emitter.
- `emit_html_runtime_to_erlang`: emit backend-owned runtime support source.
- `pretty_print`: passthrough helper.

## Core Model

1. Build a lowering context (`LowerCtx`) from module/known interfaces.
2. Lower compiler-facing syntax-output modules through `src/emit/syntax.rs`.
3. Lower CoreIR expression payloads through `src/emit/core.rs`.
4. Render `ErlForm` / `ErlModule` / `ErlExpr` nodes from `src/emit/erl.rs`
   in stable order for deterministic output.
5. Return a single source string.

`SyntaxModuleOutput` is the formal public compiler boundary. Backend lowering
internals use syntax-output declarations, expressions, and patterns directly.
Public syntax-output emission uses the strict `try_emit_*` APIs. Deprecated
syntax-output fallback wrappers and the syntax-output-to-AST module conversion
island have been retired.

The formal CLI emit path uses the transitional CoreIR-gated emitter. That entry
point validates CoreIR schema, module identity, source kind, and syntax contract
fingerprint before delegating to the strict syntax-output bridge. If bridge
lowering does not cover a construct, formal compilation fails
instead of silently adapting through the AST adapter path.

This crate is backend-specific by definition. Erlang syntax, BEAM-compatible
variable hygiene, OTP artifact layout, `.erl` rendering, and `.hrl` rendering
belong here or in a future `BackendIR[erlang]`, not in syntax output, HIR,
type checking, or backend-agnostic Core IR.

## Release-Candidate Gate

The Erlang backend is the primary release-candidate artifact path. The RC path
must enter through `try_emit_core_module_to_erlang_with_syntax_bridge`,
which validates CoreIR schema, module identity, source kind, and syntax-contract
fingerprint before producing Erlang.

`formal-erlang-core-gate` protects that CoreIR-gated entry point with focused
smoke coverage. `formal-erlang-syntax-bridge-gate` keeps the broader direct
syntax-output bridge regressions active while CoreIR expression payloads
continue to expand. The aggregate `formal-erlang-gate` runs the RC CoreIR-gated
backend smoke.

Important invariants:

- Public exports remain deterministic.
- Opaque constructors are not emitted as direct runtime constructors.
- Constructor calls are rewritten to generated helper functions.
- Backend-specific constraints must not be pushed upstream into syntax,
  resolution, type checking, or Core IR.

## Files

- `src/lib.rs`: re-exports the emitter API.
- `src/emit.rs`: public emitter entry points, syntax-output bridge
  orchestration, and backend submodule wiring.
- `src/emit/syntax.rs`: direct `SyntaxModuleOutput` to Erlang lowering used by
  strict syntax-output APIs and the transitional CoreIR-gated syntax bridge.
- `src/emit/core.rs`: CoreIR-to-Erlang expression, intrinsic, runtime
  capability, and annotation-body lowering.
- `src/emit/erl.rs`: internal Erlang form/expression/pattern/type render model.
- `src/emit/util.rs`: shared backend utility functions for type-spec lowering,
  BEAM naming, HTML escaping, constructor helper names, and trait wrapper names.
- `src/emit/runtime.rs`: embedded Erlang runtime source snippets exposed through
  the backend public API.
- `src/emit/tests.rs`: backend regression tests for direct syntax-output,
  CoreIR-gated bridge, and rendering behavior.
- `src/pretty.rs`: passthrough pretty printer helper.

## Integration Points

- Uses `terlan_syntax::SyntaxModuleOutput` on the formal compiler path.
- Does not use `terlan_syntax` source AST adapter types. Source-AST emitter
  support was removed rather than kept as a parallel backend path.
- Uses `terlan_hir::ModuleInterface` for interface-aware lowering.
- Consumed by CLI/codegen stages that write `.erl` / `.hrl` artifacts.

## Maintenance Notes

- Keep `src/emit.rs` focused on route/orchestration and declaration-level
  lowering. Move large expression, render-model, or target-capability blocks into
  `src/emit/` submodules before the file becomes difficult to review.
- Submodules must document function inputs, outputs, and transformations, even
  for private helpers.
- Prefer a small number of cohesive backend submodules over one file per helper.

## Testing Notes

- Key behavior is covered in `emit/tests.rs` for docs, exports, constructor
  lowering, aliases, CoreIR-gated emission, runtime capabilities, and
  namespace/type mapping.
- Most regressions are around constructor arity/default handling and raw/native declaration rendering.
