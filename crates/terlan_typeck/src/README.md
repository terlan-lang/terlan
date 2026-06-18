# Terlan Typecheck Source Internals

This directory owns type checking, trait validation, receiver method dispatch,
import/type visibility validation, and CoreIR lowering. It receives syntax
output plus HIR resolution metadata and produces diagnostics and formal
compiler-path artifacts.

## Responsibilities

- Validate expression, pattern, declaration, trait, constructor, and receiver
  method semantics.
- Load imported signatures and type aliases from module interfaces.
- Enforce primitive surface and target-neutral type rules.
- Lower checked modules toward CoreIR and proof-track artifacts.

## Public Surface

- `type_check_syntax_module_output`: type-checks a syntax module.
- CoreIR exports from `core_ir`.
- Diagnostic, type, and pretty-printing exports from `types`.
- Raw macro and unsupported raw declaration diagnostics.

## Core Model

Type checking builds shared inputs from resolved module metadata, imports,
aliases, trait signatures, constructors, receiver methods, and template
schemes. Expression and declaration checkers consume that context and emit
diagnostics. CoreIR modules are produced through formal lowering paths.

The main flow is:

1. Receive `SyntaxModuleOutput` and `ResolvedModule`.
2. Collect imports, aliases, signatures, traits, constructors, and receivers.
3. Check declarations and expression bodies.
4. Emit diagnostics and CoreIR-compatible artifacts where requested.

Important invariants:

- Syntax output is not mutated by type checking.
- Target/backend-specific emission does not happen here.
- Traits and receiver methods must remain coherent across imports and local
  declarations.

## Integration Points

- `terlan_syntax`: supplies source syntax output.
- `terlan_hir`: supplies resolved symbols and module interfaces.
- `terlan_erlang` and JS emitters: consume checked/CoreIR artifacts.
- CLI commands: run type checking for check, build, test, and REPL paths.

## Edge Cases

- Imported private types must not leak through public APIs.
- Receiver mutability must match method declarations and call sites.
- Trait default methods and explicit implementations must remain coherent.
- CoreIR lowering must preserve source identity and syntax contract metadata.

## Types And Interfaces

`Type`
: Internal type model used by inference and checking.

`Diagnostic`
: Typechecker diagnostic with severity and span.

`CoreModule`
: Formal lowered module artifact.

## Testing Notes

- Tests live in adjacent `*_test.rs` files.
- Add focused tests for each semantic rule before widening std usage.
- CoreIR lowering tests should stay separate from user-facing diagnostics
  tests.
