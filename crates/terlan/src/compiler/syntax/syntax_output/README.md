# Terlan Syntax Output Internals

This directory owns conversion from parsed syntax into the stable
`Syntax*Output` data model consumed by HIR, type checking, CLI commands, and
tests. It is the formal compiler boundary immediately after parsing.

## Responsibilities

- Build syntax-output declarations, expressions, patterns, types, imports, and
  module metadata.
- Preserve spans and documentation comments needed by downstream diagnostics
  and generated interfaces.
- Keep syntax-output serialization stable enough for contract tests.
- Avoid backend-specific or target-specific decisions.

## Public Surface

- Public `Syntax*Output` types are re-exported from `terlan_syntax`.
- `parse_module_as_syntax_output`: source module syntax-output entry point.
- `parse_interface_module_as_syntax_output`: interface syntax-output entry
  point.
- `parse_expr_as_syntax_output`: expression syntax-output entry point.

## Core Model

Syntax output is a backend-neutral DTO layer. It preserves what source said,
including docs, annotations, imports, declarations, expressions, patterns, and
type syntax. Later phases decide what names mean and which targets support the
forms.

The main flow is:

1. Receive parsed syntax nodes from the parser.
2. Convert each source area into `Syntax*Output` values.
3. Preserve spans, docs, and source kind metadata.
4. Return syntax-output structures for HIR/typecheck.

Important invariants:

- Syntax output records source shape without semantic resolution.
- Output structures must remain deterministic for fixture/contract tests.
- Backend details must not leak into syntax-output types.

## Integration Points

- `parser`: supplies parsed syntax nodes.
- `terlan_hir`: consumes module, import, declaration, and signature output.
- `terlan_typeck`: consumes expression, pattern, type, and declaration output.
- CLI formal gates: compare syntax-output fixtures and contracts.

## Edge Cases

- Documentation comments must attach to the declaration they document.
- Native annotations are represented as metadata, not executed.
- Interface modules have a narrower output shape than source modules.

## Types And Interfaces

`SyntaxModuleOutput`
: Parsed module DTO.

`SyntaxDeclarationPayload`
: Declaration payload enum.

`SyntaxExprOutput`
: Parsed expression DTO with source span.

## Testing Notes

- Add syntax-output tests in adjacent top-level `syntax_output_*_test.rs`
  files.
- Contract fixture updates should be deliberate and reviewed.
- Keep tests separate from implementation modules.
