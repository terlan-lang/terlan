# Documentation Validation Helpers

## Purpose

This directory contains focused validation helpers for `terlc doc`. The parent
`validation.rs` module owns command-facing error types and shared doc-block
walking. Submodules here own validation concerns that are large enough to keep
separate.

## Inputs

- Syntax-output modules with preserved documentation lines.
- Original source text for diagnostic offsets.
- Compiler profile and native-policy settings for executable examples.

## Outputs

- Parsed documentation example models.
- Command-facing validation errors with source offsets and lengths.

## Transformation

- Convert documentation comments into typed validation inputs.
- Execute REPL-backed examples through the compiler REPL evaluator.
- Preserve command-level diagnostics without coupling the doc command to parser
  internals.

## Files

- `repl_examples.rs`: extracts and validates `@example` REPL documentation
  blocks.
