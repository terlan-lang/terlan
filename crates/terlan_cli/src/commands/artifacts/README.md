# CLI Artifacts Internals

This directory owns CLI helpers for inspecting and validating generated compiler
artifacts. The implementation is centered on release-facing files produced by
compiler commands. Its most important boundary is that callers may depend on
stable artifact checks, while command routing stays outside this module.

## Responsibilities

- Read generated artifact metadata for CLI commands.
- Keep artifact diagnostics human-readable and deterministic.
- Avoid coupling artifact inspection to scratch roadmap files.

## Public Surface

- `mod.rs`: command-facing artifact validation entry points.

## Core Model

The module treats artifacts as compiler outputs that can be checked without
re-running every compiler phase.

The main flow is:

1. Receive a command request and path.
2. Read the relevant artifact metadata.
3. Return success or a stable CLI diagnostic.

Important invariants:

- Missing artifacts must fail with a clear path.
- Artifact checks must not mutate release outputs.
- Internal-only files must not become release requirements.

## Integration Points

- `crates/terlan_cli/src/commands`: routes command execution here.
- Generated compiler outputs: provide the files inspected by this module.

## Testing Notes

- Add focused command tests near the command module when artifact behavior
  changes.
