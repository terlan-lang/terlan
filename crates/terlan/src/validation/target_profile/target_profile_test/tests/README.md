# Target Profile Test Internals

This directory owns focused target-profile validation tests. It covers how
source features, std imports, and formal progression gates are accepted or
rejected for selected compiler targets.

## Responsibilities

- Validate direct CoreIR shape rules for target profiles.
- Validate standard-library bridge rules across targets.
- Cover A0 progression rules that remain relevant to target capability checks.
- Keep target-profile validation tests separate from implementation modules.

## Public Surface

- `direct_core_shape_test.rs`: target-profile rules for direct CoreIR shapes.
- `std_bridge_test.rs`: std module compatibility across target profiles.
- `a0_progression_test.rs`: formal progression compatibility coverage.

## Core Model

Target profiles describe what a target can compile. These tests assert that
unsupported language or std forms fail before backend artifact emission and
that supported forms remain accepted.

The main flow is:

1. Build a target-profile validation input.
2. Run target-profile validation.
3. Assert accepted forms or stable rejection diagnostics.

Important invariants:

- JS-only std modules must fail for non-JS targets.
- Browser-only DOM modules must fail for shared JS targets.
- Unsupported CoreIR shapes must fail before backend emission.

## Integration Points

- `validation::target_profile`: target capability validator.
- `std.js` and generated DOM modules.
- Formal pipeline tests that depend on target-profile gating.

## Edge Cases

- Target aliases such as `js` and `js.shared` must behave consistently.
- Imports can imply target-specific requirements.
- Diagnostics should mention the rejected capability clearly.

## Types And Interfaces

Target-profile tests use Rust test functions only; there is no public runtime
interface in this directory.

## Testing Notes

- Add new tests here when target capability behavior changes.
- Keep command parsing tests in CLI command/main test modules.
- Keep backend emission tests in backend or build-command test modules.
