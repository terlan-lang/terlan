# [Component Name] Internals

This directory owns [short description of the component responsibility]. The
implementation is centered on [core model or data flow]. Its most important
boundary is [what callers may depend on and what stays internal].

## Responsibilities

- [Primary responsibility.]
- [Secondary responsibility.]
- [Important validation, parsing, lowering, emission, or runtime concern.]
- [Important error handling, cleanup, compatibility, or performance concern.]

## Public Surface

- `[TypeOrFunction]`: [what callers use it for].
- `[TypeOrFunction]`: [what callers use it for].

## Core Model

[Describe the central types, state, and transformations. Explain which state is
owned here and which state is derived from upstream compiler phases.]

The main flow is:

1. [First important step.]
2. [Second important step.]
3. [Final output, diagnostic, artifact, or state update.]

Important invariants:

- [Invariant that protects correctness.]
- [Invariant that protects target boundaries.]
- [Invariant that protects diagnostics, artifacts, or cleanup.]

## Integration Points

- `[upstream module]`: [input contract].
- `[downstream module]`: [output contract].
- `[tool/runtime/API]`: [external behavior preserved or wrapped].

## Edge Cases

- [Compatibility behavior or intentionally broad handling.]
- [Failure mode and how it is surfaced.]
- [Case that must not emit partial output.]

## Types And Interfaces

`[TypeName]`
: [Purpose and key contract.]

`[InterfaceName]`
: [What implements or consumes it.]

## Testing Notes

- [Primary test module or fixture.]
- [Regression area that should get a focused test when changed.]
- [Manual or integration behavior that is hard to assert in unit tests.]
