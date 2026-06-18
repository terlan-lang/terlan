# [terlan_typeck] Internals

This package owns type checking for Terlan modules. It takes compiler-facing
syntax output plus a resolved module and returns diagnostics (errors + warnings)
by building type schemes, resolving imports/aliases, and checking expressions,
patterns, and control-flow contracts.

## Responsibilities

- Check function signatures and infer expression/pattern types.
- Enforce trait declaration invariants, constructor eligibility, expression
  typing, and config macro placeholder policy for the formal compiler path.
- Validate constructor calls, structs, lists, maps, unions, and literals.
- Resolve and validate type aliases, imports, and constructor visibility.
- Report diagnostics with spans and severity.

## Public Surface

- `type_check_syntax_module_output`: syntax-output entrypoint used by the
  compiler pipeline, returns `Vec<Diagnostic>`.
- `Type`: internal type graph (`Named`, `Union`, `Tuple`, `List`, `Map`, `Function`, etc.).
- `Diagnostic`: compiler-facing error/warning payload.
- `DiagSeverity`: `Error` or `Warning`.
- `TypeVarId`: type-variable identifier.

## Core Model

1. Collect aliases/signatures and constructor/type metadata from the module.
2. Merge with resolved symbols and interface map.
3. Infer/check each declaration and pattern/branch against expected schemes.
4. Unify inferred/expected types and normalize results.
5. Emit diagnostics for mismatches, missing signatures, invalid trait shapes,
   and unsupported source forms before backend emission.

Key invariants:

- Trait declarations and selected conformance paths are validated for method
  shape consistency.
- Exports/imports must be resolvable and visible.
- Constructor/arity and exhaustiveness checks are enforced conservatively.
- Type checking is backend independent. It must validate Terlan semantics
  without encoding Erlang syntax, BEAM-only lowering choices, OTP artifact
  layout, JS/native lowering choices, or backend hygiene rules.
- Workflow, server, protocol, and platform runtime behavior belong in target
  profiles, libraries, or generated runtime artifacts, not source-level raw
  declaration checks.

## Scheduling And Ordering

- Entire pass is synchronous.
- Signature/import diagnostics are produced before deep expression inference.
- Diagnostics are appended as checks discover issues; ordering follows module traversal.

## Integration Points

- Uses `terlan_syntax` syntax output. The typechecker does not import
  `terlan_syntax::ast` types on the compiler path.
- Uses `terlan_hir` resolved modules/interfaces and signatures.
- Consumed by `terlan_cli` for `check`/`doc` style tooling and by codegen for
  backend-agnostic typed guarantees.

## Edge Cases

- Unknown trait targets, invalid trait arity, and duplicate conformances are
  explicit errors.
- Pattern matching coverage (exhaustiveness) can return warnings/errors.
- Non-existent exported symbols and bad alias/import references are diagnosed.

## Types And Interfaces

`Type`
: type lattice used by inference/unification.

`Diagnostic`
: emitted result for every discovered issue.

`ResolvedModule`
: input contract for symbols/imports/interfaces.

## Files

- `src/lib.rs`: all type checker implementation and inline test matrix.

## Testing Notes

- Large inline test corpus validates arithmetic, ADTs, traits, constructors,
  workflows, and machine/state callbacks.
- Changes should preserve snapshot-like behavior of major failure messages and
  warning/error classification.
