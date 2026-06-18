# Main CLI Test Internals

This directory owns top-level CLI routing and command-transition tests. These
tests validate user-visible command behavior that crosses individual command
module boundaries.

## Responsibilities

- Validate top-level help and argument routing.
- Cover command transition gates and formal phase smoke paths.
- Test target-profile parsing and progression behavior.
- Keep broad CLI behavior tests separate from command implementation files.

## Public Surface

- `help_test.rs`: top-level usage and hidden internal command coverage.
- `target_profile_test.rs`: target-profile argument behavior.
- `command_transition_test.rs`: command migration and routing behavior.
- `check_*_test.rs`: formal check/phase regression coverage.
- `doc_test.rs`, `emit_js_test.rs`, `interface_test.rs`, and
  `static_site_test.rs`: cross-command smoke areas.

## Core Model

Top-level tests focus on what a user or release gate observes from `terlc`.
They should not duplicate narrower command-module tests unless the behavior is
specifically about top-level routing.

The main flow is:

1. Build CLI arguments or temporary source inputs.
2. Invoke the top-level parser/runner path.
3. Assert command selection, diagnostics, or artifact behavior.

Important invariants:

- Internal scratch commands stay hidden from normal user help.
- Target aliases resolve consistently across commands.
- Top-level routing remains stable as command modules are split.

## Integration Points

- `main.rs`: CLI entry and routing behavior.
- `commands`: command implementations under test.
- Release preflight exact tests for user-facing CLI regressions.

## Edge Cases

- Help output should stay concise and user-facing.
- Unknown commands should fail without exposing internal scratch commands.
- Target-profile errors must point at the user-selected command context.

## Types And Interfaces

Main CLI tests use Rust test functions only; there is no public runtime
interface in this directory.

## Testing Notes

- Put command-local behavior in that command's own test module.
- Use this directory for top-level routing, help, and cross-command behavior.
- Keep tests deterministic and independent of network access.
