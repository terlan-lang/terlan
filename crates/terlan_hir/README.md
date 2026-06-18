# [terlan_hir] Internals

This package owns typed module interface construction and name-resolution for the Terlan compiler pipeline. It turns parsed syntax into symbol tables, interfaces, and diagnostics that downstream stages (type checker and emitter) rely on.

## Responsibilities

- Resolve modules into `ResolvedModule` with functions, type visibility, and imports.
- Build and serialize `ModuleInterface` for incremental compilation and dependency checks.
- Load and merge interfaces from `*.typi` / `*.terli` files and standard library interface folders.
- Detect duplicate declarations, export/import mismatches, and private-access violations.

## Public Surface

- `resolve_syntax_module_output` / `resolve_syntax_module_output_with_interfaces`:
  syntax-output based symbol and diagnostic resolution.
- `syntax_module_output_to_interface`: extract public API signatures from the
  compiler-facing syntax output without exposing AST nodes.
- `load_interfaces_from_dir`, `load_interfaces_from_file_set`: interface loading helpers.
- `parse_interface_file`: parse a single interface artifact into model form.
- `ModuleInterface` and `ResolvedModule`: primary shared contracts for other crates.

## Core Model

1. Iterate declarations and collect symbol/type/import/imported-constructor state.
2. Resolve imports against known interfaces (builtin + provided external interfaces).
3. Build `FunctionSymbol` and constructor signatures.
4. Merge interface metadata into `ModuleInterface` and attach diagnostics.
5. Consume `SyntaxModuleOutput` for HIR interface extraction and resolution.
   Source-tree adapter entry points are not part of the compiler path.

Key invariants:

- Public exports and definitions must be coherent (`exported function is defined` check).
- Duplicate definitions emit diagnostics rather than silently overwriting.
- Opaque/public/private type visibility is preserved in interface maps.
- HIR/resolution artifacts are backend independent. They must not encode Erlang
  syntax, BEAM naming rules, OTP artifact layout, or emitter-only hygiene
  decisions.

## Files

- `src/lib.rs`: all public API, resolution logic, interface rendering/parsing, and tests.

## Integration Points

- Depends on `terlan_syntax` syntax output, syntax contract validation, and
  native signature parsing.
- Consumed by `terlan_cli` (incremental checks & docs), `terlan_typeck`, and
  backend lowering through backend-agnostic interfaces.
- Diagnostic format is consumed by CLI/typecheck layers.

## Edge Cases

- Missing imported interfaces produce diagnostics (`cannot find interface for module ...`).
- Private types imported as if public are rejected.
- Exported names without definitions are reported as errors.
- Interface serialization includes normalization for deterministic ordering and formatting.

## Cleanup

- No process-global mutable state; resolution is pure with in-memory maps per run.
- Read/write helpers use local accumulators and return through function outputs.

## Types And Interfaces

`ModuleInterface`
: public/private type maps, constructor/function signatures, docs, and metadata.

`ResolvedModule`
: resolved symbols plus import map and diagnostic list.

`ResolveResult`
: wrapper returning the resolved module.

`Diagnostic`
: parseable error/span payload for downstream reporting.

## Testing Notes

- Unit tests validate interface rendering, constructor summaries, and native signature extraction.
- Resolution/import edge cases are covered where exports/imports are malformed.
