# Config Contract Validation Internals

This module owns validation for Terlan config declarations after formal syntax
output is available. Config declarations are accepted source metadata, but the
generic 0.0.1 compiler path must not silently treat config entries as backend
semantics.

## Responsibilities

- Detect config declarations whose syntax-output payload contains structured
  metadata entries.
- Emit warning diagnostics that the generic compiler path preserves those entries
  without consuming them.
- Keep target-specific config behavior owned by explicit target validators.
- Keep config validation out of `main.rs` and command routing modules.

## Public Surface

- `check_config_declarations_syntax_output`: scans one syntax-output module and
  returns config contract diagnostics.

## Core Model

Config declarations appear in syntax output as `SyntaxDeclarationPayload::Config`
with a config name, target, preserved source text, structured entry list, and
declaration span. Syntax output parses the metadata block into typed config
values. This validator does not interpret those entries. It only detects that
entries exist and records that their semantics are not active in the generic
compiler path.

The main flow is:

1. Iterate syntax-output declarations.
2. Select declarations whose payload is `Config`.
3. Detect a non-empty structured entry list.
4. Return warning diagnostics anchored to the config declaration span.

Important invariants:

- Config entries are warnings, not errors, until a selected target validator
  declares them unsupported or consumes them.
- Structured syntax output is the source of truth for generic config-entry
  presence; preserved text remains available for target-specific validators and
  diagnostics.
- This module never mutates syntax output, CoreIR, or target-profile state.

## Integration Points

- `formal_pipeline`: appends config diagnostics to the normal typecheck phase.
- `commands/check`: appends the same diagnostics for directory and single-file
  check flows.
- `terlan_syntax`: provides `ConfigDecl` syntax-output payloads.
- Target validators: future owners of target-specific config semantics.

## Testing Notes

- Module-local tests cover metadata-entry detection.
- Command-level tests cover phase-manifest visibility for A0.32 release gating.
