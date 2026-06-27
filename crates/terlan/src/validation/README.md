# CLI Validation Internals

This directory owns validation helpers used by CLI commands after parsing and
phase execution. The implementation is split by validation domain so command
handlers in `main.rs` can delegate checks without carrying tool-specific
validation details.

## Responsibilities

- Keep CLI validation behavior out of the command dispatch file.
- Group validators by the artifact or contract they check.
- Preserve CLI-facing error strings and path context.
- Avoid coupling validation helpers to command parsing or output rendering.

## Public Surface

- `static_output`: validates generated static HTML and CSS artifacts.
- `config_contract`: validates syntax-output config declarations against the
  0.0.1 rule that config entries are preserved metadata unless a target-specific
  validator consumes them.
- `template_contract`: validates syntax-output template declarations against
  external template files, slots, component tags, and struct field paths.
- `target_profile`: validates lowered CoreIR against backend capability profiles
  after formal lowering.
- `proof_baseline`: stores the static LP8 proof-baseline fixture contract used
  by formal proof gates.

Public values exposed to callers are intentionally narrow and module-scoped.
Callers should import the validator they need rather than adding broad
validation prelude imports.

## Core Model

Each validation module accepts already-built inputs from a command workflow and
returns `Result<(), String>`. The command remains responsible for deciding when
validation runs and how an error maps to an exit code.

The main flow is:

1. A command parses flags and produces an artifact or input set.
2. The command calls a domain-specific validation helper.
3. The helper delegates to the relevant lower-level crate or local checker.
4. The command prints the returned message and exits.

Important invariants:

- Validators do not parse CLI flags.
- Validators do not print or exit directly.
- Errors include enough path or artifact context for command output.
- Every function, including private helpers, documents its inputs, output, and
  transformation behavior.

## Integration Points

- `crates/terlan/src/main.rs`: command handlers call validation modules.
- `crates/terlan/src/formal_pipeline.rs`: `target_profile` validation is the
  backend-gate point before formal CoreIR artifacts are returned.
- Domain feature modules such as `terlan_html`: perform syntax- or artifact-specific
  checks.
- Tests in `main.rs`: currently cover command integration while validation code
  is being extracted from the large CLI file.

## Testing Notes

- Run focused command tests for each validation domain after moving helpers.
- Preserve existing error text unless the test update is intentionally part of
  the behavior change.
- Add module-local tests when validation logic becomes more complex than simple
  delegation and formatting.
