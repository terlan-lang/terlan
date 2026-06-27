# Terlan Erlang Utility Internals

This directory owns focused helper submodules used by the Erlang emitter.

## Responsibilities

- Keep Terlan type-text lowering separate from general backend utility code.
- Preserve deterministic Erlang module, type, atom, and spec spelling.
- Keep helper visibility scoped to the Erlang emit module.

## Public Surface

- `type_specs`: Terlan type text normalization and Erlang spec lowering.

## Core Model

The utility layer provides small deterministic transformations used by syntax
bridge lowering, CoreIR lowering, runtime helper emission, and Erlang rendering.
It does not parse full Terlan modules and does not perform semantic type
checking.

## Integration Points

- Parent `emit::util` re-exports helper functions inside `crate::terlan_erlang::emit`.
- `emit::syntax` uses type/name helpers while lowering source-level forms.
- `emit::erl` uses type parameter and module-name helpers during rendering.

## Edge Cases

- Type text helpers must split only at top-level delimiters.
- Atom singleton type text must stay language-neutral before backend rendering.
- Module and type spelling must not leak target-specific casing back into
  Terlan source semantics.

## Types And Interfaces

`lower_type_to_spec`
: Converts Terlan type text into the Erlang type render model.

`map_module_name`
: Converts Terlan module paths into Erlang module atom spelling.

`map_type_name`
: Converts Terlan type names into Erlang type-spec spelling.

## Testing Notes

- Add behavior regressions through adjacent Erlang emit tests.
- Keep this layer free of direct `erlc` assumptions.
