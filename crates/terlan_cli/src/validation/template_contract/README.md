# Template Contract Validation Internals

This module owns validation for Terlan static template declarations after the
formal syntax-output parser and resolver have run.

## Responsibilities

- Run regular syntax-output typechecking and append template-specific
  diagnostics.
- Validate external template files referenced by `template` declarations.
- Check declared props, reserved `children`, slot paths, and component tags.
- Keep template validation out of `main.rs` and command routing modules.

## Public Surface

- `type_check_syntax_module_output_with_templates`: combines normal typecheck
  diagnostics with template contract diagnostics.

## Core Model

Template declarations are parsed from formal syntax output. External template
files are normalized through `commands::artifacts` into template frontend
inputs: declaration name, source path, resolved path, props, source span, and
parsed `terlan_html` template. This module then checks those inputs against the
declared props and struct field types available in the syntax output.

## Integration Points

- `main.rs`: invokes this validator during formal phase compilation.
- `commands/check`: invokes this validator during incremental directory checks.
- `commands/static_site`: owns the shared reserved `children` slot name.
- `commands::artifacts`: resolves and parses external template files.
- `terlan_html`: provides template node structures.

## Testing Notes

Existing integration tests still exercise this module through `check`, `emit`,
and static-site command flows. Add module-local tests when template contract
rules grow beyond the current focused checks.
