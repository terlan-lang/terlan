# Terlan HIR Source Internals

This directory owns module interface loading and syntax-output resolution. It
is the bridge between parsed source syntax and type checking: imports,
exports, public/private type visibility, constructors, receiver methods, trait
signatures, and interface summaries are normalized here.

## Responsibilities

- Build a resolved module view from syntax output.
- Load and render `.terli` interface summaries.
- Resolve imports against builtin and external module interfaces.
- Record diagnostics for unresolved or invalid cross-module references.

## Public Surface

- `resolve_syntax_module_output`: resolves a module with builtin interfaces.
- `resolve_syntax_module_output_with_interfaces`: resolves with caller-provided
  external interfaces.
- `load_interfaces_from_file_set`: discovers interface summaries near source
  files.
- `ModuleInterface`: stable cross-module metadata consumed by type checking.

## Core Model

HIR resolution does not own expression semantics. It creates the symbol and
interface context needed by later phases, including function signatures,
constructor signatures, trait declarations, trait conformances, and imported
type visibility.

The main flow is:

1. Receive syntax output from `terlan_syntax`.
2. Merge builtin and external interfaces.
3. Collect local declarations and exported symbols.
4. Resolve imports and emit diagnostics.
5. Return a `ResolvedModule` for type checking.

Important invariants:

- Interface summaries are normalized before consumers depend on them.
- Private type visibility is enforced through resolved metadata.
- HIR does not perform backend-specific lowering.

## Integration Points

- `terlan_syntax`: supplies `SyntaxModuleOutput`.
- `terlan_typeck`: consumes `ResolvedModule` and `ModuleInterface`.
- `terlan_cli`: loads interfaces during check, build, test, and LSP paths.
- `interface_render`: owns deterministic interface text rendering helpers.

## Edge Cases

- Missing external interfaces should produce diagnostics instead of panics.
- Function overloads must remain grouped by name and arity.
- Receiver method mutability must be preserved for type checking and lowering.

## Types And Interfaces

`ModuleInterface`
: Cross-module public/private declaration summary.

`ResolvedModule`
: Module symbol table and diagnostic bundle for type checking.

`FunctionSymbol`
: Resolved callable metadata with visibility and receiver details.

## Testing Notes

- HIR-focused tests should live beside this source module when it is split.
- Interface rendering changes need deterministic text fixtures.
- Import, visibility, trait, and constructor resolution should remain covered by
  exact compiler tests.
