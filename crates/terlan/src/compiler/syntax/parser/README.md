# Terlan Parser Internals

This directory owns recursive-descent parser modules split by source-language
area. It parses tokens into internal syntax structures while preserving spans
for diagnostics and downstream syntax-output conversion.

## Responsibilities

- Parse module declarations, imports, callables, type declarations, and traits.
- Parse expressions, patterns, HTML/template forms, and type syntax.
- Preserve precedence and associativity rules from the canonical grammar.
- Keep parser code split by language area rather than one large file.

## Public Surface

- Parser entry points are re-exported through `crates/terlan/src/compiler/syntax/mod.rs`.
- Submodules expose parser helpers to the parent parser implementation only.

## Core Model

The parser is hand-written recursive descent. Each submodule owns a coherent
syntax area and cooperates through the parent parser state. The parser accepts
Terlan source syntax only; it does not resolve names or validate target
capabilities.

The main flow is:

1. Receive a token stream from the lexer.
2. Parse declarations and nested syntax forms.
3. Preserve source spans on parse tree nodes.
4. Return parse errors with stable expected-token diagnostics.

Important invariants:

- Expression precedence must remain explicit in parser structure.
- Keyword/braced forms close according to the canonical EBNF.
- Parser modules must not import backend or typechecker crates.

## Integration Points

- `lexer`: supplies tokens.
- `syntax_output`: converts parsed structure into compiler DTOs.
- Syntax parser tests: validate accepted and rejected source forms.

## Edge Cases

- Method calls, function-value calls, field access, and indexing share postfix
  syntax and must remain unambiguous.
- Constructor patterns and normal call expressions are context-sensitive.
- Interface parsing and source parsing share most syntax but not all
  declarations.

## Types And Interfaces

`Parser`
: Internal parser state and cursor.

`ParseResult`
: Parser result alias for syntax nodes and parse diagnostics.

`ParserError`
: User-facing parse error with message and span.

## Testing Notes

- Add parser tests in adjacent top-level `parser_*_test.rs` files.
- Keep new syntax tests focused by language area.
- Do not add inline tests to parser implementation modules.
