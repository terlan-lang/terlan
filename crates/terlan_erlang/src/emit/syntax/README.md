# Terlan Erlang Syntax Bridge Emit Internals

This directory owns the transitional syntax-output-to-Erlang lowering bridge.
It exists while CoreIR becomes executable enough to drive the backend directly.

## Responsibilities

- Lower syntax-output declarations, calls, collections, constructors, imports,
  intrinsics, lets, patterns, sequences, and type values to Erlang.
- Preserve existing build/test behavior during CoreIR migration.
- Keep syntax bridge code separate from CoreIR emit helpers.
- Provide stable behavior for source features already supported by 0.0.x
  releases.

## Public Surface

- `lower_syntax_module_output`: module-level bridge lowering entry point.
- `lower_syntax_struct_headers_to_hrl`: struct header bridge lowering.
- Submodules for calls, collections, construction, declarations, imports,
  intrinsics, patterns, receiver types, and sequences.

## Core Model

The syntax bridge maps parsed source shape directly to Erlang backend
structures. It is intentionally transitional: new formal backend work should
prefer CoreIR lowering when CoreIR carries the required executable information.

The main flow is:

1. Receive `SyntaxModuleOutput` plus interfaces and imported artifacts.
2. Lower declarations and expressions by syntax category.
3. Produce an Erlang module representation for rendering.

Important invariants:

- Bridge behavior must stay deterministic while it exists.
- New backend-agnostic semantics belong in CoreIR/typecheck, not this bridge.
- User-facing 0.0.x behavior must not regress during migration.

## Integration Points

- `terlan_syntax`: supplies syntax-output structures.
- `terlan_hir`: supplies imported module interfaces.
- Parent `emit`: renders the lowered Erlang module.

## Edge Cases

- Constructor aliases and receiver calls depend on interface metadata.
- Template and file imports require artifact maps from the CLI.
- Unsupported source forms should fail before partial source is emitted.

## Types And Interfaces

`lower_syntax_module_output`
: Main syntax bridge lowering function.

`generic_dispatch`
: Helper logic for resolving source calls to emitted Erlang calls.

`receiver_types`
: Helper logic for receiver method lowering.

## Testing Notes

- Keep bridge tests separate from CoreIR emit tests.
- Add focused tests for every syntax form that remains bridge-owned.
- Mark migration opportunities in roadmap work, not as backend shortcuts.
