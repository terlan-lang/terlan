# Function Head Migration Diagnostic Policy

This document is the compatibility policy for function-head pattern migration
diagnostics across all targets in 0.0.7.

## Reserved Namespace

The migration diagnostic namespace is reserved for function-head rewrite
behavior. These IDs are stable:

- `migration.function_head_pattern.invalid_alias_style`
- `migration.function_head_pattern.safe_reject`
- `migration.function_head_pattern.unsupported_backend`

No implicit numeric fallback codes may appear for these diagnostics.

## CLI Formats

Every legacy-rejecting form maps to a stable code and stable family in all CLI
formats.

The text format must expose the same migration ID as the JSON format.
The JSON format must expose the same migration ID as the text format.
CI and tools can assert exact migration outcomes from either format.

## Target Profiles

VM allows all accepted rewrite-safe patterns.
JS target emits explicit unsupported-migration diagnostics when rewrite would
alter backend behavior.

The unsupported backend policy family is
`migration.function_head_pattern.unsupported_backend`.

## Tooling Surfaces

Editor, lsp, formatter, and tree-sitter smoke outputs must all expose the same migration IDs.
They must do so for the same source shape.

## Compatibility Matrix

The policy drift compatibility matrix has these columns:

- `parser_accept`
- `typecheck_diagnose`
- `formatter_stable`
- `vm_lower`
- `js_reject`

The matrix rows cover:

- parser acceptance plus typecheck warning migration row plus VM runtime parity
- strict profile escalation from warning to error with the same migration ID
- JS profile-specific rejection with explicit policy-family diagnostic

## Release Decision

Any migration diagnostic emitted without the reserved namespace fails the gate.
Policy matrix columns cannot be added or removed without a roadmap update and
executable snapshot update.

The `function-head-migration-lint-check` gate generates the reusable migration
manifest consumed by CLI, editor, formatter, and future codemod tooling. The
manifest records each migration ID, diagnostic family, source shape, suggested
rewrite, docs anchor, and executable test anchor.
