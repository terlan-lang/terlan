# [terlan_syntax] Internals

This package owns language syntax handling: lexical analysis, parsing, AST modeling,
and formatter/utility helpers for Terlan modules and interface modules.

## Responsibilities

- Tokenize input source into typed tokens with spans and errors.
- Parse `.terl` and `.terli` content into a typed `Module` AST.
- Compile canonical EBNF into a compiler-facing grammar contract via
  `validated_canonical_terlan_syntax_contract`, preserving rule/node ids and
  source spans.
- Validate the canonical syntax contract shape expected by downstream compiler phases.
- Expose `parse_module_as_contract` / `parse_interface_module_as_contract` for
  maintainer syntax-contract validation. These adapters emit
  `EbnfGrammarContract` and are not an end-user compile path.
- Expose `SyntaxModuleOutput` as the first compiler-facing replacement surface
  for direct `ast.rs` consumption during the EBNF-output migration.
- Provide AST types used by resolver/type-checker/emitter stages.
- Parse and extract `native core module` signatures and metadata.
- Normalize and format modules for deterministic output.

## Public Surface

- `validated_canonical_terlan_syntax_contract`: compile and validate the embedded
  canonical Terlan EBNF into `EbnfGrammarContract`.
- `cached_canonical_terlan_syntax_contract`: validate the embedded canonical EBNF
  once per compiler process and return the cached `EbnfGrammarContract` artifact.
- `cached_canonical_terlan_syntax_contract_artifact`: return the cached contract
  wrapped in a versioned compiler-facing artifact with a deterministic fingerprint.
- `cached_canonical_terlan_syntax_contract_artifact_json`: serialize that artifact
  as deterministic JSON for future generated/distributed compiler inputs.
- `cached_canonical_terlan_syntax_contract_identity`: return the compact
  schema/fingerprint identity used by phase manifests and cache keys.
- `cached_canonical_terlan_syntax_contract_identity_json`: serialize the compact
  identity object for compiler artifacts that should not embed the full grammar.
- `syntax_contract_identity_from_fingerprint`: construct the canonical
  schema/algorithm/fingerprint identity for fingerprint-only artifacts.
- `syntax_contract_identity_matches_current`: compare a manifest/cache identity
  against the compiler's current canonical syntax contract identity.
- `syntax_contract_fingerprint`: compute the stable fingerprint for an
  `EbnfGrammarContract`.
- `extract_syntax_contract_artifact_fingerprint`: read a fingerprint from either
  a raw fingerprint file or a serialized syntax contract artifact with matching
  schema and fingerprint algorithm.
- `check_syntax_contract_artifact_against_current`: classify a saved
  artifact/fingerprint as matching the compiler, mismatched, or invalid.
- `syntax_contract_artifact_matches_current`: compare a saved artifact/fingerprint
  against the compiler's cached canonical syntax contract as a compatibility
  boolean helper.
- `ensure_canonical_syntax_contract_valid`: validate the embedded canonical EBNF
  once per compiler process and reuse the cached result.
- `canonical_terlan_syntax_contract`: compile the embedded canonical Terlan EBNF
  without applying validation.
- `validate_syntax_contract`: verify that an `EbnfGrammarContract` exposes the
  required declaration, expression, pattern, call, and type-reference rules.
- `compile_ebnf` / `compile_ebnf_contract`: parse arbitrary EBNF into
  `EbnfGrammarContract`.
- `parse_module` / `parse_interface_module`: parse full modules and interfaces.
- `parse_module_as_syntax_output` / `parse_interface_module_as_syntax_output`:
  parse source into compiler-facing syntax output containing syntax-contract
  identity, source kind, module metadata, declaration classes, declaration
  docs, declaration payload summaries, raw block text, type/signature metadata,
  constructor body text, recursive expression/pattern body trees, and the
  EBNF-shaped `EbnfGrammarContract`.
- `format_module`: serialize an AST back into source text.
- `lex`: tokenize source into `Token` stream.
- `extract_native_function_signatures`, `extract_native_module_name`, `extract_native_scheduler`.
- Public AST/data types: `Module`, `Decl`, `Expr`, `Pattern`, `BinaryOp`, `Span`, `Token`, `TokenKind`.
- Re-exported parser/formatting/lexer/native APIs from `lib.rs`.

## Core Model

1. `cached_canonical_terlan_syntax_contract` validates the embedded canonical EBNF
   once per compiler process and exposes the compiler-facing contract artifact.
   Public parser entry points use the same cache as a migration guardrail.
2. Source input enters lexer (`lex`) producing tokens + `LexError` with spans.
3. Parser consumes tokens via `Parser` and builds `Module` + declaration nodes.
4. Consumers walk AST and use token/span/decl/type information for downstream checks and codegen.
5. For compiler integration, the embedded canonical EBNF is compiled into
   `EbnfGrammarContract` rules with stable ids, expression kinds, and source spans.
6. Syntax contract validation checks the grammar rules downstream compiler phases
   currently require before those phases consume the contract.
7. During migration, the contract parser translates parser declaration classes
   into the same `EbnfGrammarContract` shape (`ModuleDecl`, `ImportDecl`,
   `TypeDecl`, etc.) with `Program` as the entry rule.
8. `SyntaxModuleOutput` is the compiler-facing syntax boundary. It does not
   expose `ast.rs` types to downstream consumers. Declaration payloads currently cover
   names, declaration-site visibility, import metadata, interface export
   summaries, arities/counts, and raw/template metadata, plus type/signature
   summaries for functions, constructors, structs, traits, and templates. The
   output also carries docs, raw block text,
   constructor body text, and recursive function pattern/guard/body expression
   trees so downstream phases can start migrating away from public AST node
   imports.

Key invariants:

- Parsing must keep parser position and span alignment for reliable diagnostics.
- Canonical EBNF runtime validation is cached per process and must not run once
  per source file or compile unit.
- The syntax contract artifact identity (`schema`, `fingerprint_algorithm`, and
  `fingerprint`) is deterministic compiler input; downstream phases should
  compare that identity instead of reparsing EBNF.
- Module declarations are normalized into explicit enum variants (`Import`, `Type`, `Struct`, `Function`, ...).
- Interface parsing reuses the same AST with stricter allowed declaration shapes.

## Files

- `src/ast.rs`: AST types for modules, declarations, expressions, patterns, and operators.
- `src/ebnf.rs`: canonical EBNF parser and spanned grammar contract output.
- `src/lexer.rs`: tokenizer with syntax and doc-comment handling.
- `src/parser.rs`: recursive descent parser and parser errors.
- `src/parser_contract.rs`: temporary/early adapter that converts parsed modules
  into the grammar contract shape while the AST adapter parser is retired.
- `src/syntax_contract.rs`: embedded canonical Terlan EBNF source and contract loader.
- `src/syntax_output.rs`: compiler-facing syntax output wrapper used to migrate
  downstream phases away from direct AST consumption.
- `src/native.rs`: native block parsing helpers.
- `src/formatter.rs`: AST-to-source pretty/text formatter.
- `src/span.rs`: `Span` span model.
- `src/token.rs`: token and token-kind definitions.
- `src/lib.rs`: crate entrypoints and re-exports.

## Integration Points

- Input consumed by `terlan_cli` and all other compiler crates.
- Input contracts consumed by `terlan_hir` (resolution) and `terlan_typeck` (typing).
- `terlan_erlang` uses parser/native extraction for constructor/function lowering behavior.

## Edge Cases

- Unterminated strings/binary literals and unrecognized characters emit lex errors.
- Parser enforces dotted names, declaration ordering rules, and interface/module grammar differences.
- Doc comments are tokenized separately for module/item doc extraction and formatter round-trips.

## Cleanup

- Parser outputs remain deterministic; the only parser-level process state is the
  cached canonical EBNF validation result used during migration.
- Distributed compiler builds should move this check to build/test time and consume
  a generated or embedded syntax contract artifact by default.
- Helpers return owned structures (`Module`, tokens, signatures) and errors.

## Testing Notes

- Covered by crate-local test suites in each module for lexer/parser/native formatting behavior.
- `docs/grammar/fixtures/contract/terlan_syntax_spec_contract_summary.json`
  protects canonical grammar rule shape.
- `docs/grammar/fixtures/contract/terlan_syntax_contract_artifact_summary.json`
  protects the compiler-facing syntax contract artifact schema and fingerprint.
- Regression-prone areas: doc-comment handling, raw declaration parsing, and native signature parsing.
