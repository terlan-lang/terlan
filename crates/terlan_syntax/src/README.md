# Terlan Syntax Source Internals

This directory owns lexical analysis, parsing, formatting, native metadata
parsing, syntax contract validation, and syntax-output construction. It is the
source-language boundary for the compiler and must not depend on target
backends such as Erlang, JavaScript, Oxc, Rust emission, or SafeNative runtime
code.

## Responsibilities

- Tokenize and parse Terlan source and interface modules.
- Keep parser behavior aligned with the canonical EBNF.
- Build stable syntax-output structures for downstream phases.
- Validate syntax contract artifacts and syntax-output serialization.

## Public Surface

- `parse_module_as_syntax_output`: parses source modules into syntax output.
- `parse_interface_module_as_syntax_output`: parses interface modules.
- `parse_expr_as_syntax_output`: parses expression fixtures and REPL inputs.
- Syntax contract helpers exported from `syntax_contract`.
- Formatter entry points for source and interface modules.

## Core Model

Syntax code owns source structure, spans, tokens, and syntax-output DTOs. It
does not own name resolution, type checking, CoreIR lowering, backend emission,
or target-profile validation.

The main flow is:

1. Lex source text into tokens.
2. Parse tokens into an internal parse tree.
3. Convert parse tree nodes into syntax output.
4. Validate or serialize syntax contract artifacts when requested.

Important invariants:

- Canonical grammar lives under `docs/grammar` in the published repository.
- Syntax output remains backend-neutral.
- Parser tests live outside implementation modules as adjacent `*_test.rs`
  files.

Implementation layout:

- `lexer`: tokenizes Terlan source/interface files.
- `parser`: parses Terlan source/interface files into the internal parse tree.
- `ebnf_lexer`: tokenizes canonical EBNF contract source for `ebnf`.
- `ebnf`: parses and serializes the canonical EBNF grammar contract.
- `syntax_output`: converts internal parse-tree nodes to stable syntax-output
  DTOs for downstream compiler phases.
  - `syntax_output::model`: syntax-output DTO structs and enums.
  - `syntax_output::text`: source-like expression/type text rendering helpers
    used by generated summaries and diagnostics.
- `formatter`: formats parsed Terlan modules and interface summaries back to
  canonical source text.
  - `formatter::declarations`: declaration formatting for types, structs,
    constructors, functions, methods, traits, impls, templates, and raw
    declarations.

## Integration Points

- `docs/grammar/TERLAN_SYNTAX_SPEC.ebnf`: canonical syntax contract.
- `terlan_hir`: consumes syntax output for resolution.
- `terlan_typeck`: consumes syntax output plus resolved module metadata.
- CLI commands: parse source for check, build, test, format, and REPL paths.

## Edge Cases

- Syntax errors must preserve useful spans.
- Interface parsing must reject source-only declarations when appropriate.
- Native annotations are parsed as syntax metadata, not executed here.

## Types And Interfaces

`Token`
: Lexed source token with kind and span.

`Span`
: Byte-span source location.

`SyntaxModuleOutput`
: Backend-neutral parsed module representation.

## Testing Notes

- Parser tests live in `parser_*_test.rs`.
- Syntax-output tests live in `syntax_output_*_test.rs`.
- Grammar/contract drift is checked by `make check` and syntax contract tests.
