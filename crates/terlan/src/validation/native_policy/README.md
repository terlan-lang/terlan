# Native Policy Validation Internals

This module owns validation for CLI native-code policy enforcement. The
implementation in `mod.rs` is centered on a small `NativePolicy` enum and
lightweight source scans that run before deeper compiler phases or native
metadata emission.

## Responsibilities

- Represent the native policy selected by CLI flags.
- Reject unsafe-native declarations in safe compiler modes.
- Reject native declarations when commands request pure source only.
- Keep native-policy errors as CLI-ready strings.

## Public Surface

- `NativePolicy`: command state value describing native-code allowance.
- `NativePolicy::as_str`: stable string spelling for metadata output.
- `validate_native_policy`: policy gate used before checking or compiling source.
- `source_uses_native`: detects safe-native markers for command branching.
- `source_contains_unsafe_native`: detects unsafe-native markers for rejection.

## Core Model

The module has no persistent state. It treats source text as input and performs
conservative textual scans that are cheap enough to run before parsing or before
native metadata extraction.

The main flow is:

1. CLI argument parsing stores a `NativePolicy` in command state.
2. A command reads source text.
3. The command calls `validate_native_policy` or a lower-level detector.
4. The command prints returned policy errors and exits when validation fails.

Important invariants:

- Native-policy validation does not parse CLI flags.
- Native-policy validation does not print or exit directly.
- Unsafe-native markers are rejected even when safe-native policy is enabled.
- Every function, including private helpers, documents its inputs, output, and
  transformation behavior.

## Integration Points

- `CliState`: stores the selected policy.
- `run_check`, REPL seed loading, native metadata emission, and formal compile
  helpers: call this module before accepting native declarations.
- Native metadata generation: uses `NativePolicy::as_str` for stable JSON output.

## Edge Cases

- A source that declares `target erlang with safe_native` counts as native usage.
- Lines whose trimmed text starts with `native ` count as native declarations.
- Unsafe spelling variants are rejected by substring scan before parsing.

## Testing Notes

- `native_metadata_check_enforces_policy` protects validation rejection and
  acceptance behavior.
- Native metadata JSON tests protect `NativePolicy::as_str`.
- Add module-local tests if detection rules become more than simple textual
  scans.
